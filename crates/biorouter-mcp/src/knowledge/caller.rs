//! The caller identity every knowledge-base **listing** filters on — one value,
//! both axes, one predicate.
//!
//! # Why this module exists (audit finding 17, second spelling)
//!
//! One rule governs whether a caller may reach a knowledge base:
//! [`tier::assert_reachable`](super::tier::assert_reachable). It reads DR-15's
//! master toggle, the tier axis and DR-26's affiliation axis, in that order, and
//! it is the barrier.
//!
//! Every *listing* is the same question asked in the other direction — "which
//! bases may I name to this caller?" — and finding 17 is what happens when a
//! listing answers it with its own predicate instead of asking the barrier. It
//! happened twice, in two crates, with two different subsets of the axes:
//!
//! | surface | asked | consequence |
//! |---|---|---|
//! | `KnowledgeServer`'s five listings | tier only | a cross-institution chat saw the name of a base `kb_read_page` then refused |
//! | `resolve_target_kb`'s candidate list (`biorouter`) | tier only, **and not the toggle** | the ingest tool handed a cross-institution caller the one argument that makes the explicit-`kb_id` branch reachable; with tiers OFF it hid bases the very next call would serve in full |
//! | `Catalog::discover` (`agent_drafter`) | tier + toggle | the app catalogue named a base whose content CP3/CP4 then refused |
//!
//! The fix is never "add the missing axis to this filter too" — that is how the
//! spellings came to differ in the first place, and a fourth axis would repeat
//! it. There is no independent predicate anywhere: **the barrier answers, and
//! the filters ask the barrier.**
//!
//! # Why it is a struct
//!
//! The same reason `KnowledgeServer`'s file-private `CallerIdentity` is one: a
//! signature that takes `(bool, &CallerAffiliation)` lets a call site pass one
//! caller's tier and another's institution, and the wrong one compiles. Passing
//! the pair as one value means a future filter cannot silently drop an axis —
//! there is no narrower thing to pass. A third axis, if DR-26 ever grows one, is
//! a field here and every filter gets it at once.
//!
//! [`Self::default`] is the **restrictive** pair (public + `Unstated`), so a
//! caller assembled by omission is the least privileged one, not the most.

use super::affiliation::CallerAffiliation;
use anyhow::Result;
use std::path::Path;

/// Both axes of one caller's identity, sampled together.
///
/// ⚠ **Build it from ONE read of the provider.** Two `provider()` resolutions —
/// or a tier off the request meta and an affiliation off a cached value — is the
/// two-reads race `CallCapability` and `ProviderCompleter::paired` exist to
/// prevent, in miniature: a chat re-bound between the reads is gated on one
/// model's tier and another model's institution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KbCaller {
    private: bool,
    affiliation: CallerAffiliation,
}

impl KbCaller {
    /// Both axes, stated.
    pub fn new(private: bool, affiliation: CallerAffiliation) -> Self {
        Self {
            private,
            affiliation,
        }
    }

    /// The least privileged caller this type can express: a public model with no
    /// stated affiliation.
    ///
    /// The same value [`Default`] gives, named so a call site that means "we
    /// know nothing about this caller" says so. It is what an in-crate unit
    /// test, a caller with no request context, and an older daemon that sends
    /// neither meta key all resolve to — matching
    /// [`super::server`]'s `CallerIdentity::from_context(None)`, which fails
    /// closed on both fields.
    pub fn restricted() -> Self {
        Self::default()
    }

    pub fn is_private(&self) -> bool {
        self.private
    }

    pub fn affiliation(&self) -> &CallerAffiliation {
        &self.affiliation
    }

    /// **The** question. `Err` is the refusal, with the words the user reads.
    ///
    /// Delegates the WHOLE decision — including DR-15's master toggle — to
    /// [`tier::assert_reachable`](super::tier::assert_reachable). A caller that
    /// read the toggle itself *and* called this would be the two-reads race in
    /// miniature; that is why `discover_kbs`' own master-toggle conjunct is gone
    /// rather than kept beside this.
    pub fn assert_reachable(&self, root: &Path, kb_id: &str) -> Result<()> {
        super::tier::assert_reachable(root, kb_id, self.private, &self.affiliation)
    }

    /// [`Self::assert_reachable`], as a predicate — for the filters.
    ///
    /// ⚠ `is_ok()` and never a re-derived condition, for finding 17's reason:
    /// "omit" and "refuse" must not be able to disagree about what is reachable.
    /// Apply it **per id**, not once over a whole set — one unreachable base
    /// must not cost the caller every other one.
    pub fn can_reach(&self, root: &Path, kb_id: &str) -> bool {
        self.assert_reachable(root, kb_id).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn ucsf() -> CallerAffiliation {
        CallerAffiliation::Institution("ucsf".to_string())
    }

    /// A caller nobody stated is the least privileged one, on BOTH fields. An
    /// argument struct that forgot an axis must fail closed.
    #[test]
    fn the_default_caller_is_the_restrictive_one() {
        let caller = KbCaller::default();
        assert!(!caller.is_private());
        assert_eq!(caller.affiliation(), &CallerAffiliation::Unstated);
        assert_eq!(KbCaller::restricted(), caller);
    }

    /// The predicate is the barrier, negated — including on the affiliation
    /// axis, which is the axis finding 17's filters could not see.
    #[test]
    fn can_reach_answers_both_axes() -> Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("public-kb", "Public", None)?;
        svc.create_base("omop", "OMOP", None)?;
        crate::knowledge::tier::raise_unlocked(tmp.path(), "omop", true)?;
        crate::knowledge::tier::raise_affiliation_unlocked(tmp.path(), "omop", &ucsf())?;

        let public = KbCaller::restricted();
        assert!(public.can_reach(tmp.path(), "public-kb"));
        assert!(!public.can_reach(tmp.path(), "omop"), "tier axis");

        // The tier axis alone would say yes to both of these; only the
        // affiliation axis separates them.
        let stanford = KbCaller::new(true, CallerAffiliation::Institution("stanford".to_string()));
        assert!(stanford.can_reach(tmp.path(), "public-kb"));
        assert!(!stanford.can_reach(tmp.path(), "omop"), "affiliation axis");

        let ucsf_caller = KbCaller::new(true, ucsf());
        assert!(ucsf_caller.can_reach(tmp.path(), "omop"));

        // Both directions, or the predicate is just "refuse everyone".
        let local = KbCaller::new(true, CallerAffiliation::Local);
        assert!(local.can_reach(tmp.path(), "omop"));
        Ok(())
    }

    /// The predicate really is the barrier, negated — not a re-derived condition
    /// that happens to agree on the rows above.
    ///
    /// ⚠ DR-15's master toggle is deliberately NOT exercised here.
    /// `privacy_tiers_enabled()` is a process-global atomic and `cargo test`
    /// runs this crate's unit tests in parallel threads of one process, so
    /// flipping it would make the ~200 other knowledge tests read `false` while
    /// asserting nothing. The OFF column for every listing that reaches this
    /// predicate lives in the dedicated single-process binaries —
    /// `biorouter/tests/privacy_toggle.rs` (rows CP5 and the conversation-ingest
    /// candidate list) — for the reason that file's own header states. What is
    /// asserted here is that there is nothing *else* to read the toggle: the
    /// whole decision is delegated.
    #[test]
    fn can_reach_is_assert_reachable_negated_and_nothing_else() {
        let src = include_str!("caller.rs");
        let production = src
            .split("#[cfg(test)]")
            .next()
            .expect("caller.rs has a production half above its tests");
        // Assembled at runtime so this test's own text cannot satisfy it.
        let re_spelling = concat!("tier::", "is_private");
        assert_eq!(
            production.matches(re_spelling).count(),
            0,
            "the carrier re-spelled the reachability question instead of delegating \
             to the barrier — that is exactly how finding 17 happened"
        );
        assert_eq!(
            production.matches("privacy_tiers_enabled").count(),
            0,
            "the toggle is read once, inside tier::assert_reachable; a second read \
             here is the two-reads race in miniature"
        );
        let body = production
            .split("pub fn can_reach")
            .nth(1)
            .expect("can_reach still exists")
            .split("\n    }")
            .next()
            .expect("a closing brace");
        assert!(
            body.contains("self.assert_reachable(root, kb_id).is_ok()")
                && !body.contains("&&")
                && !body.contains("||"),
            "can_reach must be assert_reachable, nothing more, got:{body}"
        );
    }
}
