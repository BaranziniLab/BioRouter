//! One diagnostic type, and the two things that produce it: validating a single
//! draft page before it is written, and linting a whole base.
//!
//! ## Why there is a third diagnostic struct
//!
//! There are already two, and they are both right where they are.
//! [`okf::Diagnostic`] carries `rule`/`severity`/`message` and nothing else,
//! because OKF's rules are statements about **one document** and a caller
//! holding that document does not need to be told which one it is.
//! [`biookf::Finding`] adds `subject` and `path`, because BioOKF's rules are
//! bundle-scoped — "`identifier` X is duplicated" is unactionable without them.
//!
//! What neither is, is a **wire type**. Both spell `rule` as `&'static str`, so
//! neither can round-trip through JSON, and the lint macro's report is
//! serialized into a sub-agent's prompt and (Stage 6) into an HTTP response.
//! This module is that surface: the same four fields Stage 4 asks for — a
//! stable rule id, a severity, the subject the finding is about, and a message —
//! owned, `Serialize + Deserialize`, and produced by `From` impls so a new rule
//! in either layer arrives here without an edit.
//!
//! ## DR-7 all the way down
//!
//! Nothing in this module rejects anything. [`Severity`] is
//! [`okf::Severity`]'s three variants and has no fatal one, and both entry
//! points return a `Vec` rather than a `Result`-shaped verdict. A caller that
//! wants to *refuse* — which DR-7 permits for a **producer** action and for
//! nothing else — asks [`Diagnostics::errors`] and decides for itself.

use crate::knowledge::{
    biookf::{self, BundleIndex},
    okf::{self, ConceptDoc, Page},
    store,
    types::KbFormat,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// How much a diagnostic list may grow before it is cut.
///
/// A lint report is serialized into a sub-agent's prompt, so an uncapped list
/// over a thousand-page base is a context-window failure that reads as a model
/// problem. [`Diagnostics::total`] keeps the true count, so the cut is visible
/// rather than a quiet undercount.
pub const MAX_DIAGNOSTICS: usize = 200;

/// Three severities, no fatal one — see the module header.
///
/// A distinct type from [`okf::Severity`] rather than a re-export, because this
/// one crosses the wire: it is `Deserialize` and `rename_all = "lowercase"`, and
/// pinning that spelling here means a rename upstream cannot silently change a
/// JSON payload the UI reads.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A conformance condition is not met.
    Error,
    /// A SHOULD is not followed. The page is still conformant.
    Warning,
    /// A tolerance was exercised, or a check was not performed.
    Info,
}

impl From<okf::Severity> for Severity {
    fn from(s: okf::Severity) -> Self {
        match s {
            okf::Severity::Error => Self::Error,
            okf::Severity::Warning => Self::Warning,
            okf::Severity::Info => Self::Info,
        }
    }
}

/// One finding, addressed to whoever has to fix it.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable across releases. Callers, tests and the UI match on this and
    /// never on `message`, which is prose and will be reworded. Prefixed by the
    /// layer that raised it: `okf.`, `biookf.`, or `kb.` for the deterministic
    /// hygiene scan that predates both.
    pub rule: String,
    pub severity: Severity,
    /// What the finding is *about*: a page's `identifier`, a bundle-relative
    /// path, or an edge rendered as `<subject> -<predicate>-> <object>`.
    /// Never empty — a diagnostic whose subject a caller has to reconstruct
    /// from the message is one nobody acts on.
    pub subject: String,
    /// Bundle-relative path of the page, when there is one. Absent for a
    /// base-wide finding and for a draft that has not been given a path yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

impl Diagnostic {
    /// A finding raised by neither format layer: the deterministic hygiene scan
    /// in [`crate::knowledge::macros::lint`], whose four rules predate OKF and
    /// keep their behaviour unchanged.
    pub fn scan(rule: &str, severity: Severity, subject: impl Into<String>, message: &str) -> Self {
        let subject = subject.into();
        Self {
            rule: rule.to_string(),
            severity,
            path: subject.ends_with(".md").then(|| subject.clone()),
            message: message.to_string(),
            subject,
        }
    }

    /// An [`okf::Diagnostic`], which knows only its rule and its message, given
    /// the page it was raised against. The subject is supplied by the caller
    /// because the OKF layer genuinely does not have it: [`okf::check`] takes
    /// one document and has no reason to know its own name.
    pub fn from_okf_at(d: okf::Diagnostic, subject: &str, path: Option<&str>) -> Self {
        Self {
            rule: d.rule.to_string(),
            severity: d.severity.into(),
            subject: subject.to_string(),
            path: path.map(str::to_string),
            message: d.message,
        }
    }
}

impl From<biookf::Finding> for Diagnostic {
    fn from(f: biookf::Finding) -> Self {
        Self {
            rule: f.rule.to_string(),
            severity: f.severity.into(),
            subject: f.subject,
            path: f.path,
            message: f.message,
        }
    }
}

/// A capped list plus the count before the cap.
///
/// The pair, not the `Vec` alone: a truncated list that reports its own length
/// as the answer is how "3 errors" gets rendered for a base with four hundred.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostics {
    /// At most [`MAX_DIAGNOSTICS`], most severe first.
    #[serde(default)]
    pub items: Vec<Diagnostic>,
    /// How many were raised, which is `items.len()` unless the cap bit.
    #[serde(default)]
    pub total: usize,
}

impl Diagnostics {
    /// Sort by severity (errors first), then cap.
    ///
    /// Sorting **before** cutting is the whole point: a base whose first two
    /// hundred findings are `info` would otherwise report no errors at all.
    /// `sort_by_key` is stable, so within one severity the order the rules ran
    /// in survives.
    pub fn new(mut raised: Vec<Diagnostic>) -> Self {
        let total = raised.len();
        raised.sort_by_key(|d| d.severity);
        raised.truncate(MAX_DIAGNOSTICS);
        Self {
            items: raised,
            total,
        }
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.items.iter().filter(|d| d.severity == severity).count()
    }

    /// How many errors are in the **kept** list. Capped like the list itself,
    /// which is honest: an error the cut removed is one nobody was shown.
    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// True when any kept finding carries `rule`. The test-facing accessor, so
    /// a test never matches on a message string that will be reworded.
    pub fn has(&self, rule: &str) -> bool {
        self.items.iter().any(|d| d.rule == rule)
    }

    pub fn truncated(&self) -> bool {
        self.total > self.items.len()
    }
}

/// Every page in a base, parsed once.
///
/// Returned rather than folded straight into a [`BundleIndex`] because both
/// callers need the pages themselves as well: `validate` swaps one of them for
/// the draft, and the lint scan checks each against the OKF layer.
pub struct BundlePage {
    pub path: String,
    pub doc: ConceptDoc,
}

/// Read and parse every `knowledge/` page in a base.
///
/// DR-7: an unparseable page becomes a default [`ConceptDoc`] rather than an
/// error, exactly as `graph::load_pages` does — a bundle does not stop being
/// lintable because one file has an unterminated `---`. The page's own
/// unparseability is reported separately, by the OKF layer, against its text.
pub fn load_bundle(kb_root: &Path) -> Result<Vec<BundlePage>> {
    let mut out = Vec::new();
    for page in store::list_pages(kb_root, None)? {
        let text = std::fs::read_to_string(kb_root.join(&page.path))?;
        let doc = Page::parse(&text).map(|p| p.doc).unwrap_or_default();
        out.push(BundlePage {
            path: page.path,
            doc,
        });
    }
    Ok(out)
}

/// Index a bundle for the cross-document rules, with `draft` standing in for
/// whatever is on disk at its path.
///
/// The replacement is what makes duplicate-`identifier` answerable for a draft
/// **before** it is written, in both directions and without a false positive
/// either way: re-typing a page keeps one entry under its own path, while a new
/// page reusing a name that is already taken produces two and is flagged. An
/// index built from disk alone can do neither — it cannot see the draft at all.
pub fn index_with_draft(
    pages: &[BundlePage],
    draft_path: Option<&str>,
    draft: &ConceptDoc,
) -> BundleIndex {
    let replaced = draft_path.unwrap_or(DRAFT_PATH);
    BundleIndex::build(
        pages
            .iter()
            .filter(|p| p.path != replaced)
            .map(|p| (p.path.as_str(), &p.doc))
            .chain(std::iter::once((replaced, draft))),
    )
}

/// The path a draft is indexed under when the caller did not name one. A path
/// no page can occupy — `store::is_writable_page_path` requires `knowledge/` or
/// one of the three reserved names — so it can never collide with a real page
/// and silently hide it from the index.
const DRAFT_PATH: &str = "<draft>";

/// Validate one page's source text against a base's profile.
///
/// `format` is [`crate::knowledge::types::Manifest::profile`]'s answer, so a
/// **legacy** base (DR-26: below the OKF generation, `title`/`kind` frontmatter,
/// `[[wiki]]` links) is `None` and gets no format diagnostics at all. Running
/// the OKF layer over it would report `okf.type.missing` on every page of a base
/// this build has promised never to rewrite — several hundred errors describing
/// a decision, not a defect.
pub fn validate_page(
    format: Option<KbFormat>,
    path: Option<&str>,
    text: &str,
    pages: &[BundlePage],
) -> Diagnostics {
    let Some(format) = format else {
        return Diagnostics::default();
    };
    // §11 rule 1 is a question about the file, so it is asked against the text.
    // `check_source` answers it and runs the rest of the OKF layer on success.
    let parsed = Page::parse(text);
    let subject = parsed
        .as_ref()
        .ok()
        .and_then(|p| p.doc.primary_key())
        .or(path)
        .unwrap_or(UNIDENTIFIED_DRAFT)
        .to_string();
    let mut out: Vec<Diagnostic> = okf::check_source(text)
        .into_iter()
        .map(|d| Diagnostic::from_okf_at(d, &subject, path))
        .collect();
    if format.is_biookf() {
        // The profile layer needs a `ConceptDoc`, which an unparseable page does
        // not have. Reporting only the parse failure is right: every BioOKF rule
        // is a statement about frontmatter that could not be read, so running
        // them against a default doc would bury the one finding that matters
        // under twenty derived from nothing.
        if let Ok(page) = &parsed {
            let index = index_with_draft(pages, path, &page.doc);
            out.extend(
                biookf::check_doc(path, &page.doc, &index)
                    .findings
                    .into_iter()
                    .map(Diagnostic::from),
            );
        }
    }
    Diagnostics::new(out)
}

/// The subject of a finding on a draft with neither an `identifier` nor a path.
/// Distinct from [`biookf::lint::UNIDENTIFIED`]'s wording because the situations
/// differ: that one is a page in a bundle that failed to declare a key, this one
/// is a caller who has not said where the page will go.
pub const UNIDENTIFIED_DRAFT: &str = "<draft page>";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::biookf::lint::{RULE_IDENTIFIER_DUPLICATE, RULE_TYPE_INVALID};
    use crate::knowledge::okf::conformance::{RULE_FRONTMATTER_UNPARSEABLE, RULE_TYPE_MISSING};

    fn doc(frontmatter: &str) -> ConceptDoc {
        Page::parse(&format!("---\n{frontmatter}---\n\n# body\n"))
            .expect("fixture parses")
            .doc
    }

    fn bundle(pages: &[(&str, &str)]) -> Vec<BundlePage> {
        pages
            .iter()
            .map(|(path, fm)| BundlePage {
                path: (*path).to_string(),
                doc: doc(fm),
            })
            .collect()
    }

    const CITED: &str = "    knowledge_level: knowledge_assertion\n    \
                         agent_type: manual_agent\n    primary_source: DrugBank\n";

    /// DR-26. A legacy base is read through its own generation's path and is
    /// never rewritten by this build, so reporting OKF conformance against it
    /// describes a decision rather than a defect — on every page it has.
    #[test]
    fn a_legacy_base_gets_no_format_diagnostics_at_all() {
        let legacy = "---\ntitle: HRV\nkind: entity\n---\n\n[[Sleep]]\n";
        assert!(validate_page(None, Some("knowledge/hrv.md"), legacy, &[]).is_empty());
        // …and the same page under a profile is reported, so the emptiness above
        // is the profile answering `None` and not the checker doing nothing.
        let diags = validate_page(Some(KbFormat::Okf), Some("knowledge/hrv.md"), legacy, &[]);
        assert!(diags.has(RULE_TYPE_MISSING), "{:?}", diags.items);
    }

    /// OKF mode is open: an invented `type` is not even a warning (DR-7).
    #[test]
    fn okf_mode_accepts_a_type_biookf_would_flag() {
        let page = "---\ntype: Sandwich\nidentifier: A sandwich\n---\n\n# body\n";
        assert!(validate_page(Some(KbFormat::Okf), None, page, &[]).is_empty());

        let diags = validate_page(Some(KbFormat::Biookf), None, page, &[]);
        assert!(diags.has(RULE_TYPE_INVALID), "{:?}", diags.items);
    }

    /// The reason `index_with_draft` exists. Both directions, because getting
    /// one right by dropping the draft from the index gets the other wrong.
    #[test]
    fn a_draft_reusing_a_taken_identifier_is_a_duplicate_and_re_editing_its_own_page_is_not() {
        let pages = bundle(&[
            (
                "knowledge/molecule/aspirin.md",
                "type: Molecule\nidentifier: Aspirin\n",
            ),
            (
                "knowledge/disease/headache.md",
                "type: Disease\nidentifier: Headache\n",
            ),
        ]);
        let draft = "---\ntype: Molecule\nidentifier: Aspirin\n---\n\n# Aspirin\n";

        let fresh = validate_page(
            Some(KbFormat::Biookf),
            Some("knowledge/molecule/asa.md"),
            draft,
            &pages,
        );
        assert!(fresh.has(RULE_IDENTIFIER_DUPLICATE), "{:?}", fresh.items);

        let rewrite = validate_page(
            Some(KbFormat::Biookf),
            Some("knowledge/molecule/aspirin.md"),
            draft,
            &pages,
        );
        assert!(
            !rewrite.has(RULE_IDENTIFIER_DUPLICATE),
            "{:?}",
            rewrite.items
        );
    }

    /// A page whose edges resolve against the bundle reports nothing, which is
    /// what makes the tool usable during an ingest: an edge into a page that
    /// already exists must not be an error.
    #[test]
    fn an_edge_into_an_indexed_page_resolves() {
        let pages = bundle(&[
            (
                "knowledge/dataset/drugbank.md",
                "type: Dataset\nidentifier: DrugBank\nxref: [infores:drugbank]\n",
            ),
            (
                "knowledge/disease/headache.md",
                "type: Disease\nidentifier: Headache\n",
            ),
        ]);
        let draft = format!(
            "---\ntype: Molecule\nidentifier: Aspirin\nedges:\n  \
             - predicate: treats\n    object: Headache\n{CITED}---\n\n# Aspirin\n"
        );
        let diags = validate_page(
            Some(KbFormat::Biookf),
            Some("knowledge/molecule/aspirin.md"),
            &draft,
            &pages,
        );
        assert!(diags.is_empty(), "{:?}", diags.items);
    }

    /// An unparseable draft reports the parse failure and stops. The BioOKF
    /// layer is skipped rather than run against a default doc, which would
    /// otherwise bury the one actionable finding under a pile derived from it.
    #[test]
    fn an_unterminated_frontmatter_block_reports_once() {
        let diags = validate_page(
            Some(KbFormat::Biookf),
            Some("knowledge/x.md"),
            "---\ntype: Molecule\n",
            &[],
        );
        let rules: Vec<&str> = diags.items.iter().map(|d| d.rule.as_str()).collect();
        assert_eq!(rules, vec![RULE_FRONTMATTER_UNPARSEABLE]);
    }

    /// Every diagnostic names what it is about. A finding whose subject the
    /// caller has to reconstruct from prose is one nobody acts on — and the
    /// sub-agent reading this report cannot reconstruct it at all.
    #[test]
    fn every_diagnostic_carries_a_subject() {
        let draft = "---\nidentifier: A page with no type\nedges:\n  - predicate: heals\n    object: Nothing\n---\n\n# x\n";
        let diags = validate_page(Some(KbFormat::Biookf), Some("knowledge/x.md"), draft, &[]);
        assert!(!diags.is_empty());
        for d in &diags.items {
            assert!(!d.subject.is_empty(), "{d:?}");
            assert!(!d.rule.is_empty(), "{d:?}");
        }
    }

    /// A page with neither a key nor a path still gets a subject, because the
    /// alternative is an empty string in a JSON field the UI renders.
    #[test]
    fn a_keyless_pathless_draft_still_has_a_subject() {
        let diags = validate_page(Some(KbFormat::Okf), None, "no frontmatter here\n", &[]);
        assert!(!diags.is_empty());
        assert!(diags.items.iter().all(|d| d.subject == UNIDENTIFIED_DRAFT));
    }

    /// Sort-then-cut, in that order. Two hundred infos in front of one error is
    /// exactly the input where cutting first reports a clean base.
    #[test]
    fn the_cap_keeps_the_errors_and_says_how_many_it_dropped() {
        let mut raised: Vec<Diagnostic> = (0..MAX_DIAGNOSTICS + 50)
            .map(|i| Diagnostic::scan("kb.noise", Severity::Info, format!("p{i}"), "noise"))
            .collect();
        raised.push(Diagnostic::scan(
            "kb.real",
            Severity::Error,
            "knowledge/p.md",
            "the one that matters",
        ));
        let diags = Diagnostics::new(raised);
        assert_eq!(diags.items.len(), MAX_DIAGNOSTICS);
        assert_eq!(diags.total, MAX_DIAGNOSTICS + 51);
        assert!(diags.truncated());
        assert!(diags.has("kb.real"), "the cut dropped the only error");
        assert_eq!(diags.errors(), 1);
    }

    /// The reason this type exists at all: [`okf::Diagnostic`] and
    /// [`biookf::Finding`] both spell `rule` as `&'static str` and cannot come
    /// back off the wire.
    #[test]
    fn a_diagnostic_round_trips_through_json() {
        let diags = validate_page(Some(KbFormat::Okf), Some("knowledge/x.md"), "body\n", &[]);
        let json = serde_json::to_string(&diags).unwrap();
        assert_eq!(serde_json::from_str::<Diagnostics>(&json).unwrap(), diags);
        assert!(json.contains("\"severity\":\"error\""), "{json}");
    }

    /// A scan finding's `path` is filled from its subject when the subject *is*
    /// a page path, and left absent when it is a source id — the four scan rules
    /// report both kinds.
    #[test]
    fn a_scan_diagnostic_fills_path_only_when_its_subject_is_one() {
        let page = Diagnostic::scan("kb.orphan", Severity::Warning, "knowledge/a.md", "m");
        assert_eq!(page.path.as_deref(), Some("knowledge/a.md"));
        let source = Diagnostic::scan("kb.stale_source", Severity::Info, "pmid-123", "m");
        assert_eq!(source.path, None);
    }
}
