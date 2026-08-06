//! The §7 capability matrix, as one predicate per verb.
//!
//! ```text
//! VIS(T)     <=>  T <= C                      // a public caller sees public only
//! READ       <=>  VIS                         // any lineage — R6's read-only floor
//! WRITE      <=>  VIS && L in {self, child}
//! BIND(P->T) <=>  WRITE && tier(P) >= T       // Gate A, evaluated on the target
//! ```
//!
//! Design §7 is a nine-column table over three inputs. Written once, as pure
//! functions, it is unit-testable without a database and BR-71's tool handlers
//! can call it rather than re-deriving it — which is how one table becomes
//! eight slightly-different tables.
//!
//! Delegation is not amplification: VIS is evaluated against the CHILD's own
//! capability, never its parent's, so a private parent's public child sees only
//! public sessions. A public parent cannot spawn a private child at all
//! (Task 23), so it can never mint a private-capability agent to read through.
//!
//! ⚠ **These predicates shipped with ZERO production callers, and that was the
//! release blocker.** Task 21 landed the matrix; Task 20's gate recorded that no
//! task wired it; and `workspace_read_conversation` went on returning any named
//! session's whole transcript to a public-capability caller after a
//! `session_type == Hidden` check and nothing else. A code review passes that,
//! because the unit under review is correct and nothing calls it.
//!
//! They are wired now — `crates/biorouter/src/agents/workspace_extension.rs`,
//! four handlers: `workspace_read_conversation`, `workspace_list`,
//! `workspace_send_prompt` and `workspace_open` — and
//! [`tests::the_matrix_has_production_callers`] is the assertion that keeps them
//! wired. It exists specifically because "the mechanism is built, the entry point
//! is never called" is the failure this campaign has now shipped four times, and
//! every behavioural test in the world passes while it is true.
//!
//! **Still unwired, deliberately, and each for a stated reason** — §7 rules all
//! three ✗ in column C, so these are known gaps rather than decisions:
//!
//! * `workspace_watch` — its ids are not necessarily session rows. A watched id
//!   may be an in-process `BackgroundSubagent` handle with no row in the store
//!   (`session_liveness`), so a store-resolved tier gate would refuse every
//!   headless background-child watch. It returns a turn's end *reason*, not the
//!   conversation.
//! * `workspace_close` and `workspace_set_tools` — writes that return no content.
//!   §7's write row is `may_write`, i.e. VIS **and** lineage, and this change
//!   implements no lineage anywhere; wiring half of it here would make the other
//!   half look done.

use super::{visible_to, ProviderTier, SessionClassification};

/// The lineage of a target session relative to the caller, as design §7 defines
/// it: **one hop, never transitive**.
///
/// `Zelf` is spelled with a Z because `Self` is a keyword; it is the design's
/// `self` column. It is not produced by [`lineage_of`] — a caller establishes it
/// by comparing session ids before it ever looks at parentage — and it exists as
/// a variant because the matrix has a column for it and because `self` and
/// `child` behaving identically is a property worth being able to assert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lineage {
    /// The target *is* the caller's own session.
    Zelf,
    /// The caller spawned the target directly: `target.parent_session_id == caller`.
    Child,
    /// Everything else — a sibling, an unrelated session, a transitive
    /// grandchild, and any session with a NULL parent.
    Other,
}

/// Classify a target by its stored `parent_session_id` against the caller's own
/// session id.
///
/// **One hop.** A grandchild is `Other`: R6 says "sessions the caller *did*
/// spawn", and a grandchild was spawned by the child. BR-71's
/// `workspace_list { parent_session_id: "<me>" }` filter already yields exactly
/// the one-hop set, so no recursive CTE and no new "control my subtree" surface
/// is invented. A leader that needs deeper control asks its child.
///
/// A NULL parent is `Other`, i.e. read-only — the safe direction, and what every
/// session predating `parent_session_id` (Task 6) carries.
///
/// This function cannot return [`Lineage::Zelf`]: parentage does not encode
/// identity. A handler that has the target's id decides `Zelf` first:
/// `if target.id == caller { Zelf } else { lineage_of(target.parent_session_id, caller) }`.
/// Getting that wrong is not a privilege escalation — `Zelf` and `Child` are
/// merged under every rule of the matrix — but it is worth spelling out.
pub fn lineage_of(target_parent: Option<&str>, caller_session_id: &str) -> Lineage {
    match target_parent {
        Some(parent) if parent == caller_session_id => Lineage::Child,
        _ => Lineage::Other,
    }
}

/// READ ⇔ VIS, under **any** lineage — R6's read-only floor. A caller may read
/// any session it can see, whether or not it spawned it.
pub fn may_read(c: ProviderTier, t: SessionClassification) -> bool {
    visible_to(c, t)
}

/// WRITE ⇔ VIS ∧ L ∈ {self, child}. Seeing a sibling does not license steering
/// it; that is what makes column B a read-only cell rather than a refusal.
pub fn may_write(c: ProviderTier, t: SessionClassification, l: Lineage) -> bool {
    visible_to(c, t) && !matches!(l, Lineage::Other)
}

/// `workspace_list` OMITS private rows rather than redacting them: a row
/// carries a title, and a session title in this product is LLM-generated from
/// the conversation, i.e. content. Omission is one WHERE clause and removes the
/// temptation to then call read_conversation on the id.
pub fn appears_in_list(c: ProviderTier, t: SessionClassification) -> bool {
    visible_to(c, t)
}

/// A **downgrade write** — a private-capability caller writing into a public
/// target — is permitted, and discloses itself the first time it happens.
///
/// R4 explicitly permits a private session to spawn public children, and a rule
/// that lets you spawn a public child but never send it a prompt makes the
/// permission useless: it would forbid exactly the private-leader/public-worker
/// arrangement R2 names. But the prompt text *is* private-origin content
/// crossing into a public model, so the first `workspace_send_prompt` /
/// `workspace_set_tools` from a given caller into a given public target raises
/// an approval showing the exact payload.
///
/// "First" is per (caller, target) pair and is state the *caller* of this
/// predicate keeps; this function is the pure classifier of whether a crossing
/// is happening at all.
pub fn requires_first_crossing_approval(c: ProviderTier, t: SessionClassification) -> bool {
    c.is_private() && !t.is_private()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProviderTier::{Private as CPriv, Public as CPub};
    use SessionClassification::{Private as TPriv, Public as TPub};

    #[test]
    fn the_capability_matrix_matches_the_design_table_cell_for_cell() {
        use Lineage::{Child, Other, Zelf};
        // Columns A..G of design §7. `self` and `child` behave identically under
        // every rule and are merged in the table; D and F are what prove it, so
        // both are enumerated here rather than assumed.
        #[rustfmt::skip]
        let cases = [
            //  caller   target  lineage  read   write  list-visible
            ( CPub,  TPub,  Zelf,  true,  true,  true ),   // A
            ( CPub,  TPub,  Child, true,  true,  true ),   // A
            ( CPub,  TPub,  Other, true,  false, true ),   // B — R6's read-only floor
            ( CPub,  TPriv, Zelf,  false, false, false),   // C — row OMITTED, not redacted
            ( CPub,  TPriv, Child, false, false, false),   // C
            ( CPub,  TPriv, Other, false, false, false),   // C
            ( CPriv, TPub,  Zelf,  true,  true,  true ),   // D
            ( CPriv, TPub,  Child, true,  true,  true ),   // D
            ( CPriv, TPub,  Other, true,  false, true ),   // E
            ( CPriv, TPriv, Zelf,  true,  true,  true ),   // F
            ( CPriv, TPriv, Child, true,  true,  true ),   // F
            ( CPriv, TPriv, Other, true,  false, true ),   // G
        ];
        for (c, t, l, read, write, list) in cases {
            assert_eq!(may_read(c, t), read, "read {c:?}/{t:?}/{l:?}");
            assert_eq!(may_write(c, t, l), write, "write {c:?}/{t:?}/{l:?}");
            assert_eq!(appears_in_list(c, t), list, "list {c:?}/{t:?}/{l:?}");
        }
    }

    #[test]
    fn a_grandchild_is_other_and_a_null_parent_is_other() {
        // Lineage is ONE hop: R6 says "sessions the caller DID spawn", and a
        // grandchild was spawned by the child. NULL parent is `other` => read-only,
        // which is the safe direction and is what every pre-upgrade subagent has.
        assert_eq!(lineage_of(Some("me"), "me"), Lineage::Child);
        assert_eq!(lineage_of(Some("my-child"), "me"), Lineage::Other);
        assert_eq!(lineage_of(None, "me"), Lineage::Other);
    }

    /// **The assertion that would have caught the release blocker: these
    /// predicates have production callers.**
    ///
    /// The bug this replaces was an *absence*. Every unit in this module was
    /// correct, every test here passed, and `workspace_read_conversation` handed
    /// a public-capability caller a private session's whole transcript — because
    /// nothing called any of it. No behavioural test can be written against a
    /// gate that does not exist, which is exactly why the omission survived a
    /// code review; the only thing that catches it is asking whether the wiring
    /// is there at all.
    ///
    /// So this is a source scan, written to fail the two ways a source scan
    /// usually lies:
    ///
    /// * it scans only the **production** half of the file, cut at the
    ///   `#[cfg(test)]` module — otherwise the workspace tests' own fixtures
    ///   would satisfy it. The negative control below is what proves the cut
    ///   landed where it claims; without it, a `find` that returned 0 or the
    ///   end of the file would make every assertion here vacuous, which is how
    ///   this campaign has already shipped a grep gate that passed by accident;
    /// * it names the file that ships the tool surface, so moving the wiring
    ///   into a test helper, a doc comment or another crate does not keep it
    ///   green.
    ///
    /// It deliberately does **not** assert *where* in that file the calls are.
    /// Placement is what the behavioural tests in
    /// `agents::workspace_extension::tests` hold (a public caller is refused on
    /// each wired path); this holds the thing they cannot see, which is that
    /// deleting the gate leaves the predicate with no consumer again.
    #[test]
    fn the_matrix_has_production_callers() {
        const WORKSPACE: &str = include_str!("../agents/workspace_extension.rs");
        let cut = WORKSPACE
            .find("#[cfg(test)]\nmod tests {")
            .expect("workspace_extension.rs no longer has a `#[cfg(test)] mod tests`");
        let (production, tests) = WORKSPACE.split_at(cut);

        // The control, FIRST. `for_test_restricted` is spelled only by tests —
        // production capabilities come from `CallCapability::sample` or
        // `public_enforced` — so it is the marker that proves the split is real
        // in BOTH directions.
        assert!(
            !production.contains("for_test_restricted"),
            "the cut did not remove the test module, so the assertions below prove nothing"
        );
        assert!(
            tests.contains("for_test_restricted"),
            "the cut removed more than the test module"
        );

        for predicate in ["may_read(", "appears_in_list("] {
            assert!(
                production.contains(predicate),
                "`{predicate}` has no production caller in workspace_extension.rs. This is the \
                 release blocker recurring: the §7 matrix exists, the tool handlers do not ask \
                 it, and every unit test in this module still passes."
            );
        }
    }

    #[test]
    fn a_downgrade_write_is_permitted_but_flagged_for_first_crossing_approval() {
        // R4 permits a private session to spawn public children, and a rule that
        // lets you spawn one but never send it a prompt makes the permission
        // useless. The prompt text IS private-origin content crossing into a
        // public model, so the FIRST crossing per (caller,target) discloses it.
        assert!(may_write(CPriv, TPub, Lineage::Zelf));
        assert!(requires_first_crossing_approval(CPriv, TPub));
        assert!(!requires_first_crossing_approval(CPriv, TPriv));
        assert!(!requires_first_crossing_approval(CPub, TPub));
    }
}
