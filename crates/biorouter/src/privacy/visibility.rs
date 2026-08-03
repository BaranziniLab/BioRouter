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
//! ⚠ **Nothing in production calls these yet, and that is a recorded gap rather
//! than an oversight.** Design §7 rules `workspace_list` and
//! `workspace_read_conversation`, and neither handler consults `privacy_tier`
//! today; see the Task 20 note in `docs/security/privacy-tiers-execution-plan.md`.
//! The predicate ships first so that wiring is one call on data already in hand.

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
