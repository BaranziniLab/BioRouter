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
//! They are wired now — four handlers in
//! `crates/biorouter/src/agents/workspace_extension.rs`
//! (`workspace_read_conversation`, `workspace_list`, `workspace_send_prompt`,
//! `workspace_open`) and two actions of `platform__manage_schedule` in
//! `agents/schedule_tool.rs` (`session_content`, which returned any named
//! session's whole transcript, and `sessions`, which listed titles and working
//! directories) — through [`refuse_unless_readable`] below, which is the one
//! adapter both surfaces call.
//!
//! Two assertions keep them wired, and they are not redundant:
//! [`tests::the_matrix_has_production_callers`] holds the *placement* (the file
//! that ships the workspace tool surface names the gate), and the tree-wide
//! census in `crates/biorouter/tests/privacy_guard_wiring.rs` holds the general
//! property for every guard in this module — including the ones below that have
//! no caller at all, which it requires to carry a written reason. Both exist
//! because "the mechanism is built, the entry point is never called" is the
//! failure this campaign has now shipped five times, and every behavioural test
//! in the world passes while it is true.
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

/// READ, applied to a session the caller merely **named**: resolve the target's
/// classification and refuse unless [`may_read`] permits it.
///
/// ⚠ **One adapter, not one per tool.** This is the body that used to live in
/// `workspace_extension`'s `refuse_unless_visible`, lifted here when a second
/// caller appeared — `platform__manage_schedule`'s `session_content` action,
/// which returned any named session's entire transcript with no tier check at
/// all. Two handlers resolving a tier and phrasing a refusal by hand is how one
/// table becomes seven slightly-different tables; the second copy would have
/// been the one that forgot the metadata-only read, or answered "no such
/// session" separately from "private".
///
/// Three properties, each load-bearing and each inherited from the original:
///
/// * **The master opt-out is read off the same sample that carried the tier**
///   (`cap.enforced()`), never re-derived here.
/// * **A caller that may read a private conversation never touches the store**,
///   which leaves the handler's own honest errors intact for the caller
///   entitled to them.
/// * **`Err` and "could not read the row" are the same answer** (§14.4 / R10):
///   an unauthorised caller must not learn from the refusal whether the
///   conversation exists.
///
/// The row is read **metadata-only** (`with_messages: false`): resolving the
/// tier must never itself be the way to load the transcript the gate is about
/// to refuse.
pub async fn refuse_unless_readable(
    cap: super::CallCapability,
    session_manager: &crate::session::session_manager::SessionManager,
    target_session_id: &str,
) -> Result<(), String> {
    if !cap.enforced() {
        return Ok(());
    }
    // Asked THROUGH the matrix rather than as `cap.tier().is_private()`, so this
    // short-circuit can never disagree with the decision below it.
    if may_read(cap.tier(), SessionClassification::Private) {
        return Ok(());
    }
    match session_manager.get_session(target_session_id, false).await {
        Ok(session) if may_read(cap.tier(), session.privacy_tier) => Ok(()),
        // Private, and unreadable, and absent — one sentence for all three.
        _ => Err(super::refusal::workspace_out_of_reach()),
    }
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
        // The module's VISIBILITY is not this scan's business, and pinning one
        // spelling of it made this test a tripwire for an unrelated change. The
        // workspace lane made that module `pub(crate)` so a sibling file could
        // share a test helper; this `find` was looking for exactly
        // `#[cfg(test)]\nmod tests {`, stopped matching, and panicked — taking
        // the entire `biorouter` test binary down before any other test ran, so
        // one cosmetic edit hid 2443 results. Step over an optional visibility
        // modifier instead; the negative control below is what actually proves
        // the cut landed, and it does that for any spelling.
        let cut = WORKSPACE
            .match_indices("mod tests {")
            .find_map(|(i, _)| {
                // `get`, not `[..i]`: `match_indices` only ever yields char
                // boundaries so the slice cannot actually panic, but clippy's
                // `string_slice` does not know that and `-D warnings` makes it
                // an error. Asking for the option is honest either way — and
                // cheaper than an `#[allow]` that a reader has to take on faith.
                let before = WORKSPACE.get(..i)?.trim_end();
                let before = before
                    .strip_suffix("pub(crate)")
                    .unwrap_or(before)
                    .trim_end();
                let before = before.strip_suffix("pub").unwrap_or(before).trim_end();
                before
                    .ends_with("#[cfg(test)]")
                    .then(|| before.len() - "#[cfg(test)]".len())
            })
            .expect(
                "workspace_extension.rs no longer has a `#[cfg(test)]` test module, so this scan \
                 cuts the file there, so it cannot run without one",
            );
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

        // ⚠ `may_read(` is deliberately NOT one of these any more, and reading
        // this list as a weakening would be wrong. When `platform__manage_schedule`
        // turned out to be a second handler reading any named transcript, the
        // resolve-then-ask body moved into `refuse_unless_readable` above so both
        // callers ask one predicate instead of two hand-written copies. So the
        // workspace file now names the adapter rather than the matrix, and it is
        // the adapter that names `may_read`. What holds the general property —
        // every guard in the matrix has a live caller *somewhere*, through
        // whatever chain — is the tree-wide census in
        // `crates/biorouter/tests/privacy_guard_wiring.rs`, which knows the
        // difference between a call, an import and a same-named local, and which
        // would fail if this delegation ever became a dead link.
        for predicate in ["refuse_unless_readable(", "appears_in_list("] {
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
