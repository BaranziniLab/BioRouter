//! OKF v0.2 §11 conformance, as diagnostics.
//!
//! §11 has three parts and this module owns all three, even where the behaviour
//! one of them mandates lives elsewhere — a rule tested in the module that
//! *implements* it and not in the module that *owns* it is how a rule quietly
//! stops being checked.
//!
//! **The three bundle conformance conditions.** "A bundle is conformant with OKF
//! v0.2 if: (1) every non-reserved `.md` file in the tree contains a parseable
//! YAML frontmatter block; (2) every frontmatter block contains a non-empty
//! `type` field; (3) every reserved filename (`index.md`, `log.md`) follows the
//! structure in §8 and §9 respectively when present." They are conditions on a
//! *bundle*, not producer style rules — [`check`] answers (1) and (2) for one
//! page, [`check_index`] and [`check_log`] answer (3) for the two reserved
//! files.
//!
//! **The five MUST-NOT-REJECT tolerances.** "Consumers MUST NOT reject a bundle
//! because of: missing optional frontmatter fields; unknown `type` values;
//! unknown additional frontmatter keys; broken cross-links; missing `index.md`
//! files." Honoured structurally: [`Severity`] has no fatal variant, so no rule
//! in this module *can* reject anything, and `no_rule_is_fatal` pins it.
//!
//! **The three additional consumer rules.** A consumer "MUST treat a bare
//! `verified` mapping as a one-element list (§5.2)"; "MUST NOT reject a concept
//! for missing any optional family (§5.3)"; "SHOULD derive trust tiers and
//! staleness only from the fields specified here, and SHOULD surface, not
//! silently drop, a failing attestation (§10.5)". The first is implemented by
//! [`super::trust::normalize_verified`] and asserted here. The second is the
//! reason [`check`] emits nothing at all for the minimal fixture. The third is
//! half-honoured on purpose: **attested computations (§10) are not implemented
//! in this build**, and rather than say nothing, [`RULE_ATTESTATION_UNCHECKED`]
//! surfaces that when a page declares one — a bundle from a Google-side OKF
//! producer can legally contain one, and silently dropping the attestation is
//! the failure §10.5 names.
//!
//! ## Nothing here rejects (DR-7)
//!
//! A `Severity::Error` means "this page is not conformant", never "this page
//! will not be read". Producer-side strictness — `kb_write_page` refusing a
//! malformed write in BioOKF mode — is a Stage 4 decision about a *write*, and
//! DR-7 is explicit that producers are held to a higher bar than consumers.
//! Consumers, which is everything in this module, only ever describe.
//!
//! BioOKF's own rules (the 28 types, the 35 predicates, domain/range, the
//! required edge triplet, `identifier` uniqueness) are deliberately absent: the
//! vocabulary is Stage 1's, and checking it here would make the OKF profile
//! report a legal document as broken.

use super::links::{self, FootnoteKind};
use super::model::Page;
use super::{frontmatter, trust};

pub const RULE_FRONTMATTER_UNPARSEABLE: &str = "okf.frontmatter.unparseable";
pub const RULE_FRONTMATTER_ABSENT: &str = "okf.frontmatter.absent";
pub const RULE_TYPE_MISSING: &str = "okf.type.missing";
pub const RULE_SOURCE_RESOURCE_MISSING: &str = "okf.source.resource_missing";
pub const RULE_GENERATED_BY_MISSING: &str = "okf.generated.by_missing";
pub const RULE_FOOTNOTE_UNRESOLVED: &str = "okf.footnote.unresolved";
pub const RULE_VERIFIED_BARE_MAPPING: &str = "okf.verified.bare_mapping";
pub const RULE_ATTESTATION_UNCHECKED: &str = "okf.attestation.unchecked";
pub const RULE_INDEX_FRONTMATTER: &str = "okf.index.frontmatter";
pub const RULE_LOG_DATE_HEADING: &str = "okf.log.date_heading";

/// The `type` value §10 gives an attested computation. Spelled once.
const ATTESTED_COMPUTATION: &str = "Attested Computation";

/// There is no `Fatal`. See the module header: the absence is the mechanism by
/// which §11's five tolerances are honoured, not an oversight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Something §11 states as a conformance condition is not met.
    Error,
    /// A SHOULD in §5–§9 is not followed. The page is still conformant.
    Warning,
    /// A tolerance was exercised, or a check was not performed. Reported so it
    /// is visible rather than silent.
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable across releases; callers and tests match on this, never on the
    /// message, which is prose and will be reworded.
    pub rule: &'static str,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    fn new(rule: &'static str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            rule,
            severity,
            message: message.into(),
        }
    }
}

/// Check one already-parsed concept document.
///
/// Takes the whole [`Page`] and not just its frontmatter because half of §5.1's
/// provenance contract lives in the body: a `[^id]` footnote is only
/// attributable through a matching `sources[].id`, so a checker handed only the
/// frontmatter can never see an unattributed claim.
pub fn check(page: &Page) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    check_frontmatter_presence(page, &mut out);
    check_type(page, &mut out);
    check_sources(page, &mut out);
    check_generated(page, &mut out);
    check_footnotes(page, &mut out);
    note_bare_verified(page, &mut out);
    note_attestation(page, &mut out);
    out
}

/// Check a concept document from its source text, so §11 rule 1 — "contains a
/// parseable YAML frontmatter block" — is answerable at all. [`check`] alone
/// cannot report it: by the time a `Page` exists the parse has already
/// succeeded.
pub fn check_source(text: &str) -> Vec<Diagnostic> {
    match Page::parse(text) {
        Ok(page) => check(&page),
        Err(e) => vec![Diagnostic::new(
            RULE_FRONTMATTER_UNPARSEABLE,
            Severity::Error,
            format!("§11 rule 1: frontmatter is not parseable: {e}"),
        )],
    }
}

/// §11 rule 3 for `index.md` (§8): "Index files contain no frontmatter, with one
/// exception: a bundle-root `index.md` MAY carry an `okf_version` key."
///
/// `is_bundle_root` is a parameter because the exception is scoped to exactly
/// one file in the tree, and a checker that granted it everywhere would let a
/// producer scatter version keys through every subdirectory.
pub fn check_index(text: &str, is_bundle_root: bool) -> Vec<Diagnostic> {
    let Ok(split) = frontmatter::split(text) else {
        return vec![Diagnostic::new(
            RULE_INDEX_FRONTMATTER,
            Severity::Warning,
            "§8: an index file's frontmatter block is unparseable",
        )];
    };
    split
        .frontmatter
        .iter()
        .filter(|(k, _)| !(is_bundle_root && k.as_str() == Some("okf_version")))
        .map(|(k, _)| {
            Diagnostic::new(
                RULE_INDEX_FRONTMATTER,
                Severity::Warning,
                format!(
                    "§8: index.md carries frontmatter key `{}`; only a bundle-root \
                     index.md may carry `okf_version`",
                    k.as_str().unwrap_or("?")
                ),
            )
        })
        .collect()
}

/// §11 rule 3 for `log.md` (§9): "Date headings MUST use ISO 8601 `YYYY-MM-DD`
/// form."
///
/// Only the date headings are checked. §9 says the rest is convention — "Log
/// entries are prose; the leading bold word (`**Update**`, `**Creation**`) is a
/// convention, not a requirement" — so a checker that enforced the bold word
/// would report conformant logs as broken.
pub fn check_log(text: &str) -> Vec<Diagnostic> {
    text.lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(str::trim)
        .filter(|heading| chrono::NaiveDate::parse_from_str(heading, "%Y-%m-%d").is_err())
        .map(|heading| {
            Diagnostic::new(
                RULE_LOG_DATE_HEADING,
                Severity::Warning,
                format!("§9: log date heading `{heading}` is not ISO 8601 YYYY-MM-DD"),
            )
        })
        .collect()
}

fn check_frontmatter_presence(page: &Page, out: &mut Vec<Diagnostic>) {
    if !page.had_frontmatter {
        out.push(Diagnostic::new(
            RULE_FRONTMATTER_ABSENT,
            Severity::Error,
            "§11 rule 1: no `---` frontmatter block; a concept document must have one",
        ));
    }
}

fn check_type(page: &Page, out: &mut Vec<Diagnostic>) {
    if page.doc.r#type.trim().is_empty() {
        out.push(Diagnostic::new(
            RULE_TYPE_MISSING,
            Severity::Error,
            "§11 rule 2: frontmatter has no non-empty `type`; §4.1 makes it the one \
             always-required key",
        ));
    }
    // No rule fires for an *unrecognised* type: §4.1 says values "are not
    // registered centrally" and §11 forbids rejecting a bundle over one. BioOKF
    // mode flags it (DR-7); OKF mode must not even warn.
}

fn check_sources(page: &Page, out: &mut Vec<Diagnostic>) {
    for (i, source) in page.doc.sources.iter().enumerate() {
        if source.resource.as_deref().unwrap_or("").trim().is_empty() {
            let label = source.id.clone().unwrap_or_else(|| format!("[{i}]"));
            out.push(Diagnostic::new(
                RULE_SOURCE_RESOURCE_MISSING,
                Severity::Warning,
                format!(
                    "§5.1: source `{label}` has no `resource`, which is REQUIRED within an \
                     entry; without it the source names nothing a consumer can follow"
                ),
            ));
        }
    }
}

fn check_generated(page: &Page, out: &mut Vec<Diagnostic>) {
    if page.doc.generated.as_ref().is_some_and(|g| g.by.is_empty()) {
        out.push(Diagnostic::new(
            RULE_GENERATED_BY_MISSING,
            Severity::Warning,
            "§5.2: `generated` is present without `by`, which is REQUIRED within it",
        ));
    }
}

/// §5.1's join key, checked in the one direction that loses information.
///
/// A `[^id]` reference with no matching `sources[].id` is a claim that *looks*
/// attributed and is not — the footnote prose may name a paper, but §5.1 says
/// "consumers resolve attribution through the matching entry, not by parsing the
/// footnote prose", so to every consumer the claim is unsourced. The reverse
/// (a `sources` entry nothing cites) is normal and not reported: §5.1 makes `id`
/// optional and a document-level source needs no citation.
fn check_footnotes(page: &Page, out: &mut Vec<Diagnostic>) {
    let ids: Vec<&str> = page
        .doc
        .sources
        .iter()
        .filter_map(|s| s.id.as_deref())
        .collect();
    let mut reported: Vec<String> = Vec::new();
    for note in links::extract_footnote_refs(&page.body) {
        if note.kind != FootnoteKind::Reference
            || ids.contains(&note.id.as_str())
            || reported.contains(&note.id)
        {
            continue;
        }
        reported.push(note.id.clone());
        out.push(Diagnostic::new(
            RULE_FOOTNOTE_UNRESOLVED,
            Severity::Warning,
            format!(
                "§5.1: footnote `[^{}]` attributes a claim to a source id that is not in \
                 `sources`",
                note.id
            ),
        ));
    }
}

fn note_bare_verified(page: &Page, out: &mut Vec<Diagnostic>) {
    let raw = page.doc.verified.as_ref();
    let is_bare = matches!(raw, Some(super::model::VerifiedField::One(_)));
    if is_bare && !trust::normalize_verified(raw).is_empty() {
        out.push(Diagnostic::new(
            RULE_VERIFIED_BARE_MAPPING,
            Severity::Info,
            "§5.2: `verified` is a bare mapping; read as a one-element list, which §11 \
             makes a consumer MUST",
        ));
    }
}

fn note_attestation(page: &Page, out: &mut Vec<Diagnostic>) {
    if page.doc.r#type.trim() == ATTESTED_COMPUTATION {
        out.push(Diagnostic::new(
            RULE_ATTESTATION_UNCHECKED,
            Severity::Info,
            "§10: this build does not verify attested computations; the contract is \
             preserved but not checked (§10.5 asks a consumer to surface, not silently \
             drop, an unverified attestation)",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf::fixtures;
    use crate::knowledge::okf::model::{ConceptDoc, VerifiedField};

    fn rules(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.rule).collect()
    }

    fn errors(diags: &[Diagnostic]) -> Vec<&str> {
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.rule)
            .collect()
    }

    // ── §11's three bundle conformance conditions ───────────────────────────

    #[test]
    fn rule_1_unparseable_frontmatter_is_reported_from_the_source_text() {
        let diags = check_source(fixtures::UNTERMINATED);
        assert_eq!(rules(&diags), vec![RULE_FRONTMATTER_UNPARSEABLE]);
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn rule_1_an_absent_block_is_reported_separately_from_an_unparseable_one() {
        // The two are different facts and a checker that conflated them could
        // only report neither — see `frontmatter`'s module header.
        let diags = check_source(fixtures::NO_FRONTMATTER);
        assert!(rules(&diags).contains(&RULE_FRONTMATTER_ABSENT));
        assert!(!rules(&diags).contains(&RULE_FRONTMATTER_UNPARSEABLE));
    }

    #[test]
    fn rule_2_a_missing_type_is_an_error() {
        let diags = check_source("---\ntitle: No type here\n---\nbody\n");
        assert_eq!(errors(&diags), vec![RULE_TYPE_MISSING]);
    }

    #[test]
    fn rule_2_a_whitespace_only_type_is_not_a_non_empty_type() {
        let diags = check_source("---\ntype: \"   \"\n---\nbody\n");
        assert_eq!(errors(&diags), vec![RULE_TYPE_MISSING]);
    }

    #[test]
    fn rule_3_a_non_root_index_may_not_carry_frontmatter() {
        let text = "---\nokf_version: \"0.2\"\n---\n# Section\n";
        assert_eq!(
            rules(&check_index(text, false)),
            vec![RULE_INDEX_FRONTMATTER]
        );
    }

    #[test]
    fn rule_3_the_bundle_root_index_may_carry_okf_version_and_nothing_else() {
        let ok = "---\nokf_version: \"0.2\"\n---\n# Section\n";
        assert!(check_index(ok, true).is_empty());
        let extra = "---\nokf_version: \"0.2\"\ntitle: Bundle\n---\n# Section\n";
        assert_eq!(
            rules(&check_index(extra, true)),
            vec![RULE_INDEX_FRONTMATTER]
        );
    }

    #[test]
    fn rule_3_an_index_with_no_frontmatter_is_conformant() {
        assert!(check_index("# Section\n\n* [A](a.md) - a\n", false).is_empty());
        assert!(check_index("# Section\n", true).is_empty());
    }

    #[test]
    fn rule_3_log_date_headings_must_be_iso_8601() {
        let log = "# Directory Update Log\n\n## 2026-05-22\n* **Update**: x\n\n## May 15\n* y\n";
        let diags = check_log(log);
        assert_eq!(rules(&diags), vec![RULE_LOG_DATE_HEADING]);
        assert!(diags[0].message.contains("May 15"));
    }

    #[test]
    fn rule_3_log_entry_prose_is_convention_and_is_not_enforced() {
        // §9: the bold leading word is "a convention, not a requirement".
        let log = "# Log\n\n## 2026-05-22\n* just some prose with no bold word\n";
        assert!(check_log(log).is_empty());
    }

    // ── §11's five MUST-NOT-REJECT tolerances ───────────────────────────────

    #[test]
    fn tolerance_missing_optional_fields_produce_no_diagnostics_at_all() {
        // §11: "MUST NOT reject a concept for missing any optional family."
        assert!(check_source(fixtures::MINIMAL).is_empty());
    }

    #[test]
    fn tolerance_an_unknown_type_value_is_not_even_a_warning() {
        // §4.1: type values "are not registered centrally". Flagging one here
        // would make every OKF-profile base noisy for being legal.
        assert!(check_source("---\ntype: Sourdough Starter\n---\nbody\n").is_empty());
    }

    #[test]
    fn tolerance_unknown_additional_keys_are_not_reported() {
        let diags = check_source(fixtures::UNKNOWN_KEYS);
        assert!(errors(&diags).is_empty(), "got {diags:?}");
        assert!(
            !rules(&diags).iter().any(|r| r.contains("unknown")),
            "got {diags:?}"
        );
    }

    #[test]
    fn tolerance_broken_cross_links_are_not_reported() {
        // §6.1: "a link whose target does not exist in the bundle is not
        // malformed; it may simply represent not-yet-written knowledge."
        let text = "---\ntype: X\n---\nSee [nothing](/does/not/exist.md) and [[also nothing]].\n";
        assert!(check_source(text).is_empty());
    }

    #[test]
    fn tolerance_a_missing_index_is_not_this_modules_business() {
        // The fifth tolerance is about a bundle, and there is no rule id for it
        // anywhere in this module — which is the implementation.
        assert!(!rules(&check_source(fixtures::MINIMAL))
            .iter()
            .any(|r| r.contains("index")));
    }

    #[test]
    fn no_rule_is_fatal() {
        // The structural guarantee behind all five tolerances and behind DR-7:
        // the most severe thing this module can say is "not conformant".
        assert_eq!(Severity::Error.max(Severity::Warning), Severity::Warning);
        for text in fixtures::ROUND_TRIPPABLE.iter().map(|(_, t)| *t) {
            for d in check_source(text) {
                assert!(
                    matches!(
                        d.severity,
                        Severity::Error | Severity::Warning | Severity::Info
                    ),
                    "a fourth severity would need a decision about rejection"
                );
            }
        }
    }

    // ── §11's three additional consumer rules ───────────────────────────────

    #[test]
    fn consumer_must_read_a_bare_verified_mapping_as_a_one_element_list() {
        // The MUST is implemented in `trust`; it is asserted here because §11 is
        // where it is stated, and a rule tested only where it is implemented is
        // a rule nobody re-checks when the implementation moves.
        let page = super::Page::parse(fixtures::BARE_VERIFIED).unwrap();
        assert!(matches!(page.doc.verified, Some(VerifiedField::One(_))));
        assert_eq!(
            trust::normalize_verified(page.doc.verified.as_ref()).len(),
            1
        );
        assert!(rules(&check(&page)).contains(&RULE_VERIFIED_BARE_MAPPING));
    }

    #[test]
    fn consumer_surfaces_rather_than_silently_dropping_an_unchecked_attestation() {
        let diags = check_source("---\ntype: Attested Computation\n---\n# Computation\n");
        assert_eq!(rules(&diags), vec![RULE_ATTESTATION_UNCHECKED]);
        assert_eq!(diags[0].severity, Severity::Info);
    }

    // ── §5 producer SHOULDs, reported as warnings ───────────────────────────

    #[test]
    fn a_source_entry_without_a_resource_warns_and_names_the_entry() {
        let text = "---\ntype: X\nsources:\n  - id: ga4-schema\n    title: GA4\n---\nbody\n";
        let diags = check_source(text);
        assert_eq!(rules(&diags), vec![RULE_SOURCE_RESOURCE_MISSING]);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("ga4-schema"));
    }

    #[test]
    fn generated_without_by_warns() {
        let text = "---\ntype: X\ngenerated: { at: 2026-06-20T22:53:05Z }\n---\nbody\n";
        assert_eq!(rules(&check_source(text)), vec![RULE_GENERATED_BY_MISSING]);
    }

    #[test]
    fn a_footnote_with_no_matching_source_id_warns_once_per_id() {
        let text = "---\ntype: X\nsources:\n  - id: known\n    resource: https://e/x\n---\n\
                    a[^unknown] b[^unknown] c[^known]\n\n[^known]: K\n";
        let diags = check_source(text);
        assert_eq!(rules(&diags), vec![RULE_FOOTNOTE_UNRESOLVED]);
        assert!(diags[0].message.contains("unknown"));
    }

    #[test]
    fn a_source_that_nothing_cites_is_not_a_finding() {
        // §5.1 makes `id` optional; a document-level source needs no citation.
        let text = "---\ntype: X\nsources:\n  - id: unused\n    resource: https://e/x\n---\nbody\n";
        assert!(check_source(text).is_empty());
    }

    #[test]
    fn a_footnote_definition_alone_is_not_an_unresolved_reference() {
        let text = "---\ntype: X\n---\nprose\n\n[^stray]: leftover definition\n";
        assert!(check_source(text).is_empty());
    }

    // ── the fixtures ────────────────────────────────────────────────────────

    #[test]
    fn the_full_v0_2_fixture_is_conformant() {
        let diags = check_source(fixtures::FULL_V0_2);
        assert!(diags.is_empty(), "expected a clean bill, got {diags:?}");
    }

    #[test]
    fn the_biookf_worked_example_is_conformant_as_plain_okf() {
        // §1's claim, checked rather than asserted: BioOKF's own §12 example
        // must satisfy every OKF v0.2 condition, since BioOKF only adds
        // constraints. Its `edges:` are unknown keys to OKF and §11 forbids
        // reporting them.
        let diags = check_source(fixtures::TOCILIZUMAB);
        assert!(diags.is_empty(), "expected a clean bill, got {diags:?}");
    }

    #[test]
    fn a_document_with_no_frontmatter_fails_both_of_the_first_two_conditions() {
        let diags = check_source(fixtures::NO_FRONTMATTER);
        let mut got = errors(&diags);
        got.sort_unstable();
        assert_eq!(got, vec![RULE_FRONTMATTER_ABSENT, RULE_TYPE_MISSING]);
    }

    #[test]
    fn rule_ids_are_unique_across_the_module() {
        // The ids are the stable API; two rules sharing one would make a caller
        // unable to act on either.
        let mut ids = vec![
            RULE_FRONTMATTER_UNPARSEABLE,
            RULE_FRONTMATTER_ABSENT,
            RULE_TYPE_MISSING,
            RULE_SOURCE_RESOURCE_MISSING,
            RULE_GENERATED_BY_MISSING,
            RULE_FOOTNOTE_UNRESOLVED,
            RULE_VERIFIED_BARE_MAPPING,
            RULE_ATTESTATION_UNCHECKED,
            RULE_INDEX_FRONTMATTER,
            RULE_LOG_DATE_HEADING,
        ];
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before);
        assert!(ids.iter().all(|id| id.starts_with("okf.")));
    }

    #[test]
    fn check_does_not_read_the_biookf_vocabulary() {
        // Stage 1 owns the 28 types and the 35 predicates. If this module ever
        // learns them, the OKF profile starts reporting legal pages as broken.
        let d = ConceptDoc {
            r#type: "Not A BioOKF Type".into(),
            edges: vec![super::super::model::Edge {
                predicate: "invented_predicate".into(),
                object: "Nothing".into(),
                ..Default::default()
            }],
            ..ConceptDoc::default()
        };
        let page = super::Page {
            doc: d,
            body: String::new(),
            had_frontmatter: true,
        };
        assert!(check(&page).is_empty());
    }
}
