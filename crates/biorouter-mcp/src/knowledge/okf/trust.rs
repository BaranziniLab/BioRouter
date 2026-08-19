//! Trust and freshness, both **derived** and never stored.
//!
//! OKF §5.3 gives three trust tiers and §5.5 one staleness test, and both are
//! defined as functions of frontmatter the producer wrote. This module is those
//! functions and nothing else — there is deliberately no `trust_tier` field
//! anywhere in [`super::model`].
//!
//! ## Why "never stored" is a rule and not a preference
//!
//! §5.1 states the principle for credibility and it applies identically here: a
//! stored verdict "is subjective, unportable across consumers, and goes stale".
//! Concretely: a page verified by a human in June and edited by an agent in July
//! has a stored tier that still says human-reviewed, and nothing in the file
//! disagrees with it. Deriving on read makes that impossible — the tier is
//! whatever `verified` says right now.
//!
//! ## This is NOT the privacy tier
//!
//! [`crate::knowledge::tier`] also has the word "tier" and also has two values,
//! and it *is* access control: it decides whether a public model may read a
//! private base, it fails closed, and it ratchets permanently. [`TrustTier`]
//! decides nothing. §5.3 is explicit — "Trust tiers are advisory signals, not
//! access control." A future caller that reaches for a trust tier to gate a read
//! has picked up the wrong one of the two.

use super::model::{ConceptDoc, Status, Timestamp, Verified, VerifiedField};
use chrono::NaiveDate;

/// §5.3, lowest to highest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustTier {
    /// No `verified` key at all.
    Unverified,
    /// `verified` by non-`human:` actors only.
    MachineConfirmed,
    /// At least one `human:<id>` verifier.
    HumanReviewed,
}

/// Flatten `verified` into the list every consumer wants.
///
/// **This is an explicit consumer MUST**, not a convenience. §5.2: "A single
/// verifier MAY be written as one `{ by, at }` mapping without the list dash.
/// Consumers MUST treat a bare mapping as a one-element list", and §11 repeats
/// it in the conformance rules. A consumer that only understands the list form
/// reads a bare-mapping page as *unverified* — it does not error, it silently
/// downgrades the page's trust, which is the worst possible failure for a field
/// whose entire job is to say how much to trust the page.
pub fn normalize_verified(field: Option<&VerifiedField>) -> Vec<Verified> {
    match field {
        None => Vec::new(),
        Some(VerifiedField::One(v)) => vec![v.clone()],
        Some(VerifiedField::Many(v)) => v.clone(),
    }
}

/// §5.3. Absence is meaningful and is not an error: "A concept with no trust
/// frontmatter is still consumable; consumers MUST NOT reject it (§11)."
pub fn trust_tier(doc: &ConceptDoc) -> TrustTier {
    let events = normalize_verified(doc.verified.as_ref());
    if events.is_empty() {
        return TrustTier::Unverified;
    }
    if events.iter().any(|v| v.by.is_human()) {
        TrustTier::HumanReviewed
    } else {
        TrustTier::MachineConfirmed
    }
}

/// The latest `verified[].at`, which §5.2 defines as the answer to "how
/// recently".
///
/// Compared as parsed instants and not as strings: `2026-06-25T09:00:00Z` and
/// `2026-06-25T09:00:00+00:00` are the same moment and sort differently as text,
/// so a string `max` would pick by spelling. An unparseable `at` is skipped
/// rather than treated as the epoch, so one malformed entry cannot make a
/// freshly verified page look ancient.
pub fn latest_verified_at(doc: &ConceptDoc) -> Option<Timestamp> {
    normalize_verified(doc.verified.as_ref())
        .into_iter()
        .filter_map(|v| v.at)
        .filter_map(|t| t.parse().map(|parsed| (parsed, t)))
        .max_by_key(|(parsed, _)| *parsed)
        .map(|(_, t)| t)
}

/// §5.5: "A concept is stale when `today >= stale_after`."
///
/// `today` is a parameter and not `Utc::now()` so the comparison is testable and
/// so a caller rendering a whole bundle uses one date for every page — two pages
/// evaluated either side of midnight otherwise disagree about the same day.
///
/// An absent **or unparseable** `stale_after` is not stale. The unparseable case
/// is the load-bearing half: reading a typo as "stale now" would let one bad
/// character flip an entire base to expired in the UI, and §11's tolerances say
/// a malformed optional field must not be turned against the document.
pub fn is_stale(doc: &ConceptDoc, today: NaiveDate) -> bool {
    doc.stale_after
        .as_ref()
        .and_then(|d| d.parse())
        .is_some_and(|deadline| today >= deadline)
}

/// §5.4: "Absent `status` ⇒ `stable`."
///
/// Derived rather than defaulted into the model, for the same reason as the
/// trust tier: writing `stable` into a document that never claimed it turns a
/// consumer's assumption into the producer's assertion on the next write.
pub fn effective_status(doc: &ConceptDoc) -> Status {
    doc.status.clone().unwrap_or(Status::Stable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf::fixtures;
    use crate::knowledge::okf::model::{Actor, Date, Page};

    fn doc(text: &str) -> ConceptDoc {
        Page::parse(text).unwrap().doc
    }

    fn verified(by: &str, at: &str) -> Verified {
        Verified {
            by: Actor(by.into()),
            at: Some(Timestamp(at.into())),
        }
    }

    #[test]
    fn a_bare_verified_mapping_becomes_a_one_element_list() {
        // The §5.2 / §11 MUST, on a real fixture rather than a constructed value.
        let d = doc(fixtures::BARE_VERIFIED);
        let events = normalize_verified(d.verified.as_ref());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].by.as_str(), "human:ahormati");
    }

    #[test]
    fn a_bare_human_verifier_is_human_reviewed_not_unverified() {
        // The silent downgrade this module exists to prevent: a consumer that
        // only understands the list form reads this page as Unverified.
        assert_eq!(
            trust_tier(&doc(fixtures::BARE_VERIFIED)),
            TrustTier::HumanReviewed
        );
    }

    #[test]
    fn no_verified_key_is_unverified() {
        assert_eq!(trust_tier(&doc(fixtures::MINIMAL)), TrustTier::Unverified);
    }

    #[test]
    fn machine_only_verifiers_are_machine_confirmed() {
        let d = ConceptDoc {
            verified: Some(VerifiedField::Many(vec![
                verified("process:finance-nightly", "2026-06-26T02:00:00Z"),
                verified("reference_agent/gemini-2.5-pro", "2026-06-26T03:00:00Z"),
            ])),
            ..ConceptDoc::default()
        };
        assert_eq!(trust_tier(&d), TrustTier::MachineConfirmed);
    }

    #[test]
    fn one_human_among_machines_lifts_the_tier() {
        let d = ConceptDoc {
            verified: Some(VerifiedField::Many(vec![
                verified("process:finance-nightly", "2026-06-26T02:00:00Z"),
                verified("human:ahormati", "2026-06-25T09:00:00Z"),
            ])),
            ..ConceptDoc::default()
        };
        assert_eq!(trust_tier(&d), TrustTier::HumanReviewed);
    }

    #[test]
    fn an_empty_verified_list_is_unverified_not_machine_confirmed() {
        let d = ConceptDoc {
            verified: Some(VerifiedField::Many(vec![])),
            ..ConceptDoc::default()
        };
        assert_eq!(trust_tier(&d), TrustTier::Unverified);
    }

    #[test]
    fn tiers_order_lowest_to_highest() {
        assert!(TrustTier::Unverified < TrustTier::MachineConfirmed);
        assert!(TrustTier::MachineConfirmed < TrustTier::HumanReviewed);
    }

    #[test]
    fn latest_verified_at_compares_instants_not_text() {
        let d = ConceptDoc {
            verified: Some(VerifiedField::Many(vec![
                verified("human:a", "2026-06-25T09:00:00+00:00"),
                verified("process:b", "2026-06-25T08:00:00Z"),
            ])),
            ..ConceptDoc::default()
        };
        // Byte-wise "2026-06-25T09:00:00+00:00" < "2026-06-25T08:00:00Z" because
        // '+' sorts below '0'; by instant the 09:00 entry is later.
        assert_eq!(
            latest_verified_at(&d).unwrap().as_str(),
            "2026-06-25T09:00:00+00:00"
        );
    }

    #[test]
    fn an_unparseable_at_is_skipped_rather_than_treated_as_the_epoch() {
        let d = ConceptDoc {
            verified: Some(VerifiedField::Many(vec![
                verified("human:a", "yesterday"),
                verified("process:b", "2026-06-25T08:00:00Z"),
            ])),
            ..ConceptDoc::default()
        };
        assert_eq!(
            latest_verified_at(&d).unwrap().as_str(),
            "2026-06-25T08:00:00Z"
        );
    }

    #[test]
    fn stale_after_is_a_plain_date_comparison_inclusive_of_the_day_itself() {
        let d = ConceptDoc {
            stale_after: Some(Date("2026-09-23".into())),
            ..ConceptDoc::default()
        };
        let day = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert!(!is_stale(&d, day("2026-09-22")));
        assert!(is_stale(&d, day("2026-09-23")), "stale ON the day (§5.5)");
        assert!(is_stale(&d, day("2026-09-24")));
    }

    #[test]
    fn a_missing_or_malformed_stale_after_is_never_stale() {
        let day = NaiveDate::parse_from_str("2030-01-01", "%Y-%m-%d").unwrap();
        assert!(!is_stale(&ConceptDoc::default(), day));
        let typo = ConceptDoc {
            stale_after: Some(Date("2026-13-99".into())),
            ..ConceptDoc::default()
        };
        assert!(
            !is_stale(&typo, day),
            "one typo must not expire the whole base"
        );
    }

    #[test]
    fn absent_status_reads_as_stable_without_being_written_back() {
        let page = Page::parse(fixtures::MINIMAL).unwrap();
        assert_eq!(effective_status(&page.doc), Status::Stable);
        assert!(
            !page.render().contains("status:"),
            "the derived default must not be materialised into the file"
        );
    }

    #[test]
    fn an_unknown_status_word_is_reported_as_itself() {
        let d = doc("---\ntype: X\nstatus: archived\n---\n");
        assert_eq!(effective_status(&d), Status::Other("archived".into()));
    }
}
