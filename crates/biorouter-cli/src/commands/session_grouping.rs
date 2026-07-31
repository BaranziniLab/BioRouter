//! BR-71 CLI parity (operator requirement, 2026-07-30): subagent runs are
//! discoverable from a terminal, grouped under the session that spawned them.
//!
//! Deliberately the same shape as the renderer's
//! `ui/desktop/src/components/sessions/sessionGrouping.ts` (Task 38): orphans
//! stay top-level so nothing becomes unreachable, and only a `sub_agent` row
//! ever nests. Two surfaces, one rule.

use biorouter::session::session_manager::SessionType;
use chrono::{DateTime, Utc};

/// The fields a listing needs. A projection rather than `biorouter::session::
/// Session` so the pure helpers below can be tested without a store.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub session_type: SessionType,
    pub parent_session_id: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

/// Whether the daemon says this session has a turn in flight.
///
/// Three-valued, for the same reason `SessionLiveness` in
/// `workspace_extension.rs` is: with no daemon reachable the honest answer is
/// "not knowable from here", and collapsing that into `Finished` prints
/// "finished" over a run that is still going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Running,
    Finished,
    Unknown,
}

pub struct Group {
    pub session: SessionRow,
    pub children: Vec<SessionRow>,
}

/// Nest `sub_agent` rows under the parent they name, when that parent is in the
/// same page. Everything else — including a subagent whose parent was deleted or
/// falls outside the page — stays top level.
pub fn group_by_parent(rows: Vec<SessionRow>) -> Vec<Group> {
    use std::collections::{HashMap, HashSet};
    let ids: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let mut children: HashMap<String, Vec<SessionRow>> = HashMap::new();
    let mut top: Vec<SessionRow> = Vec::new();

    for row in &rows {
        let parent = match (&row.session_type, &row.parent_session_id) {
            (SessionType::SubAgent, Some(p)) if ids.contains(p.as_str()) => Some(p.clone()),
            _ => None,
        };
        match parent {
            Some(p) => children.entry(p).or_default().push(row.clone()),
            None => top.push(row.clone()),
        }
    }

    top.into_iter()
        .map(|session| {
            let mut kids = children.remove(&session.id).unwrap_or_default();
            kids.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
            Group {
                session,
                children: kids,
            }
        })
        .collect()
}

/// The machine-readable spelling of `Liveness`, for the `--format json` arm —
/// so a script gets the same three-valued answer the text arm prints, including
/// the "unknown" case a boolean could not express.
pub fn liveness_label(liveness: Liveness) -> &'static str {
    match liveness {
        Liveness::Running => "running",
        Liveness::Finished => "finished",
        Liveness::Unknown => "unknown",
    }
}

/// One indented line for a child, carrying the three things that separate two
/// siblings of one fan-out: its own label, its id, and how far it got.
pub fn render_child(row: &SessionRow, liveness: Liveness) -> String {
    let state = match liveness {
        Liveness::Running => "● live",
        Liveness::Finished => "○ done",
        Liveness::Unknown => "· state unknown (no daemon)",
    };
    format!(
        "  └─ {} [{}]  {}  {} msgs  {}",
        row.name,
        state,
        row.id,
        row.message_count,
        row.updated_at.format("%Y-%m-%d %H:%M")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, kind: SessionType, parent: Option<&str>, name: &str) -> SessionRow {
        SessionRow {
            id: id.to_string(),
            name: name.to_string(),
            session_type: kind,
            parent_session_id: parent.map(str::to_string),
            updated_at: chrono::Utc::now(),
            message_count: 3,
        }
    }

    #[test]
    fn subagents_nest_under_their_parent_and_orphans_stay_top_level() {
        let rows = vec![
            row("p1", SessionType::User, None, "Migration review"),
            row(
                "c1",
                SessionType::SubAgent,
                Some("p1"),
                "Subagent: audit the migration",
            ),
            row(
                "c2",
                SessionType::SubAgent,
                Some("gone"),
                "Subagent: benchmark",
            ),
        ];
        let grouped = group_by_parent(rows);

        let parent = grouped.iter().find(|g| g.session.id == "p1").unwrap();
        assert_eq!(
            parent
                .children
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c1"]
        );
        // A child whose parent is not in this page must remain reachable, not
        // vanish — same rule as Task 38's `groupSessionsByParent`.
        assert!(grouped.iter().any(|g| g.session.id == "c2"));
        // …and it must not ALSO appear as a child of anything.
        assert_eq!(grouped.iter().filter(|g| g.session.id == "c2").count(), 1);
        assert!(!grouped
            .iter()
            .any(|g| g.children.iter().any(|c| c.id == "c2")));
    }

    /// The discriminating half: a parent row that is itself a `sub_agent`
    /// (depth-2 delegation) must not swallow its sibling.
    #[test]
    fn a_row_is_nested_only_when_it_is_itself_a_subagent() {
        let rows = vec![
            row("p1", SessionType::User, None, "root"),
            // A USER row that happens to carry a parent id (a diverged session
            // reusing the column would look like this) is NOT a subagent.
            row("u2", SessionType::User, Some("p1"), "user chat"),
        ];
        let grouped = group_by_parent(rows);
        assert!(
            grouped.iter().any(|g| g.session.id == "u2"),
            "only session_type == sub_agent nests"
        );
    }

    #[test]
    fn a_running_row_is_rendered_differently_from_a_finished_one_and_from_an_unknown_one() {
        let mut r = row("c1", SessionType::SubAgent, Some("p1"), "Subagent: audit");
        r.message_count = 12;
        let running = render_child(&r, Liveness::Running);
        let finished = render_child(&r, Liveness::Finished);
        let unknown = render_child(&r, Liveness::Unknown);
        assert_ne!(running, finished);
        assert_ne!(finished, unknown);
        assert_ne!(running, unknown);
        // Identity: the id prefix and the message count are what separate two
        // siblings whose labels were truncated to the same prefix.
        assert!(running.contains("c1"));
        assert!(running.contains("12"));
    }
}
