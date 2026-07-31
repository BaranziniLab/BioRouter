//! BR-71 CLI parity (operator requirement, 2026-07-30): subagent runs are
//! discoverable from a terminal, grouped under the session that spawned them.
//!
//! The rule, stated once so the GUI's grouped History (Task 38) can be held to
//! the same one:
//!
//!   1. only a `sub_agent` row ever nests — `parent_session_id` alone is not
//!      enough, because a diverged user chat may carry one too;
//!   2. the tree is exactly one level deep;
//!   3. **no row is ever dropped.** Anything that cannot nest under a top-level
//!      parent — an orphan, a grandchild, a cycle — stays top level.
//!
//! ⚠ The renderer counterpart does not exist in this worktree yet, so this is
//! the rule Task 38 must be written against, not a claim that two files already
//! agree. When it lands, cite it here by path and keep the two in step.

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

/// The parent a row would nest under, ignoring for now whether that parent is
/// itself top level. `p != row.id` rejects a row that names itself as its own
/// parent.
///
/// A free function rather than a closure inside `group_by_parent`: the returned
/// `&str` borrows from `row`, and a closure cannot express that higher-ranked
/// relationship (`error: lifetime may not live long enough`).
fn wanted_parent<'a>(
    row: &'a SessionRow,
    ids: &std::collections::HashSet<&str>,
) -> Option<&'a str> {
    match (&row.session_type, &row.parent_session_id) {
        (SessionType::SubAgent, Some(p)) if p != &row.id && ids.contains(p.as_str()) => {
            Some(p.as_str())
        }
        _ => None,
    }
}

/// Nest `sub_agent` rows under the parent they name, when that parent is in the
/// same page. Everything else — including a subagent whose parent was deleted or
/// falls outside the page — stays top level.
///
/// The listing is exactly ONE level deep, so a row may only nest under a parent
/// that is itself top level. That restriction is not cosmetic: a bucket is only
/// ever read back for a row that ended up top level, so filing a row under a key
/// that itself nested writes it somewhere nobody reads and the row disappears
/// from the listing. Depth-2 delegation, a self-parenting row and a parent/child
/// cycle all reach that state. Such a row therefore stays top level — the same
/// treatment an orphan gets, and for the same reason: in a monitoring surface a
/// row rendered in a slightly less useful place beats a row that is not rendered
/// at all. Guarded by `no_row_is_ever_dropped_by_grouping`.
pub fn group_by_parent(rows: Vec<SessionRow>) -> Vec<Group> {
    use std::collections::{HashMap, HashSet};
    let ids: HashSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();

    // Every row that wants to nest. A row in here cannot also be nested UNDER,
    // which is what keeps the tree one level deep and every row reachable.
    let would_nest: HashSet<&str> = rows
        .iter()
        .filter(|row| wanted_parent(row, &ids).is_some())
        .map(|row| row.id.as_str())
        .collect();

    let mut children: HashMap<String, Vec<SessionRow>> = HashMap::new();
    let mut top: Vec<SessionRow> = Vec::new();

    for row in &rows {
        match wanted_parent(row, &ids).filter(|parent| !would_nest.contains(parent)) {
            Some(p) => children.entry(p.to_string()).or_default().push(row.clone()),
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
        // ⚠ Not "(no daemon)". A missing daemon is only one of several reasons
        // liveness can be unknown — a stripped secret, a non-200 and an
        // unreadable body reach here too — and naming the wrong one sends the
        // reader after a problem that does not exist. `handle_session_list`
        // prints the actual reason once, on stderr.
        Liveness::Unknown => "· state unknown",
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

    /// Grouping is a MONITORING surface, so its one absolute rule is that no row
    /// may disappear. A single-pass "file under my parent if my parent is on the
    /// page" does lose rows: the bucket is only ever read back for rows that
    /// ended up top level, so anything filed under a key that itself nested is
    /// written and never retrieved.
    ///
    /// Three shapes reach that state, and every one of them is silent:
    ///   * depth-2 delegation `p1 ← c1 ← g1` (`g1` files under `c1`, which is
    ///     itself filed under `p1`),
    ///   * a self-parenting row (`id == parent_session_id`),
    ///   * a two-row cycle `a ↔ b`, where neither row is ever top level.
    ///
    /// Depth-2 is prevented by policy today (`subagent_handler.rs` disables
    /// `subagents_enabled` inside a `SubAgent` session), so this is a guard
    /// against the policy changing — which is exactly when a silent drop would
    /// be hardest to notice.
    #[test]
    fn no_row_is_ever_dropped_by_grouping() {
        let reachable = |grouped: &[Group]| -> std::collections::HashSet<String> {
            grouped
                .iter()
                .flat_map(|g| {
                    std::iter::once(g.session.id.clone())
                        .chain(g.children.iter().map(|c| c.id.clone()))
                })
                .collect()
        };

        // Depth-2 delegation.
        let grouped = group_by_parent(vec![
            row("p1", SessionType::User, None, "root"),
            row("c1", SessionType::SubAgent, Some("p1"), "child"),
            row("g1", SessionType::SubAgent, Some("c1"), "grandchild"),
        ]);
        assert_eq!(
            reachable(&grouped),
            ["p1", "c1", "g1"].map(str::to_string).into_iter().collect(),
            "a grandchild must stay reachable, not vanish into a bucket nobody reads"
        );

        // A row that names itself as its parent.
        let grouped = group_by_parent(vec![row("s1", SessionType::SubAgent, Some("s1"), "self")]);
        assert_eq!(
            reachable(&grouped),
            ["s1"].map(str::to_string).into_iter().collect(),
            "a self-parenting row must not file itself under itself"
        );

        // A two-row cycle.
        let grouped = group_by_parent(vec![
            row("a", SessionType::SubAgent, Some("b"), "a"),
            row("b", SessionType::SubAgent, Some("a"), "b"),
        ]);
        assert_eq!(
            reachable(&grouped),
            ["a", "b"].map(str::to_string).into_iter().collect(),
            "a cycle must degrade to two top-level rows, not to an empty listing"
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
        // ⚠ `contains("12")` would be near-vacuous: `updated_at` is rendered as
        // `%Y-%m-%d %H:%M`, which contains "12" for a large slice of the day, so
        // that assertion passes even if the message count is dropped entirely.
        assert!(running.contains("12 msgs"));
    }
}
