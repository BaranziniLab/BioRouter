//! The profile entry point: hand it a document and the bundle it lives in, get
//! back diagnostics.
//!
//! ## It warns. It does not reject. (DR-7)
//!
//! There is deliberately no `is_valid()`, no `ok()`, and no `Result` anywhere on
//! this surface — only counts a caller may choose to act on. DR-7: "Nothing
//! anywhere rejects a page on read", and OKF §11 gives consumers five
//! MUST-NOT-REJECT tolerances that a boolean would quietly override. The one
//! place strictness belongs is a **producer** action — `kb_write_page` refusing
//! a malformed write in BioOKF mode — and that is Stage 4's decision about a
//! write, made by reading [`Report::errors`], not a property of the document.
//!
//! ## Two layers, one report
//!
//! A BioOKF bundle is an OKF bundle with extra rules, so [`check`] runs
//! [`crate::knowledge::okf::check`] first and appends the profile's own
//! findings. Rule ids keep their `okf.` / `biookf.` prefixes, so a reader can
//! always tell which layer objected — and a Stage 6 UI can offer the OKF layer
//! alone for a base that is not in BioOKF mode.
//!
//! The OKF layer needs the body (§5.1's footnotes are only attributable through
//! the frontmatter) and the profile layer does not, which is why there are two
//! entry points rather than one with an `Option<&str>` body.

use super::lint::{self, BundleIndex, Finding, Severity};
use crate::knowledge::okf::{self, ConceptDoc, Page};

/// A lint run's findings, plus the counts a caller actually branches on.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }

    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }

    pub fn infos(&self) -> usize {
        self.count(Severity::Info)
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// True when any finding carries `rule`. The test-facing accessor, so a test
    /// never has to match on a message string that will be reworded.
    pub fn has(&self, rule: &str) -> bool {
        self.findings.iter().any(|f| f.rule == rule)
    }

    /// Every finding carrying `rule`, in the order they were raised.
    pub fn matching<'a>(&'a self, rule: &'a str) -> impl Iterator<Item = &'a Finding> {
        self.findings.iter().filter(move |f| f.rule == rule)
    }
}

/// Check one page: OKF v0.2 conformance, then the BioOKF profile.
pub fn check(path: Option<&str>, page: &Page, index: &BundleIndex) -> Report {
    let subject = page
        .doc
        .primary_key()
        .unwrap_or(lint::UNIDENTIFIED)
        .to_string();
    let mut findings: Vec<Finding> = okf::check(page)
        .into_iter()
        .map(|d| Finding {
            rule: d.rule,
            severity: d.severity,
            subject: subject.clone(),
            path: path.map(str::to_string),
            message: d.message,
        })
        .collect();
    findings.extend(lint::check_page(path, &page.doc, index));
    Report { findings }
}

/// Check frontmatter alone, for a caller that has a [`ConceptDoc`] and no body
/// — a draft being validated before it is written, for instance.
///
/// Skips the OKF layer rather than running half of it: §5.1's footnote rule and
/// §11's rule 1 are both statements about the file, and reporting "no
/// frontmatter block" for a document that was handed over already parsed would
/// be a finding about the caller, not the content.
pub fn check_doc(path: Option<&str>, doc: &ConceptDoc, index: &BundleIndex) -> Report {
    Report {
        findings: lint::check_page(path, doc, index),
    }
}

/// Check a whole bundle: build the index once, then every page against it.
///
/// The index is built from the same slice, so the duplicate-`identifier` and
/// unresolved-`object` rules see exactly the pages being checked — a lint run
/// over a subset would otherwise report every edge that leaves the subset as
/// broken.
pub fn check_bundle(pages: &[(&str, Page)]) -> Report {
    let index = BundleIndex::build(pages.iter().map(|(path, page)| (*path, &page.doc)));
    let mut findings = Vec::new();
    for (path, page) in pages {
        findings.extend(check(Some(path), page, &index).findings);
    }
    Report { findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::biookf::lint::{
        RULE_EDGE_CONTRADICTION, RULE_EDGE_OBJECT_UNRESOLVED, RULE_IDENTIFIER_DUPLICATE,
        RULE_TYPE_INVALID,
    };

    fn page(frontmatter: &str) -> Page {
        Page::parse(&format!("---\n{frontmatter}---\n\n# body\n")).expect("fixture parses")
    }

    const CITED: &str =
        "    knowledge_level: knowledge_assertion\n    agent_type: manual_agent\n    \
         primary_source: DrugBank\n";

    fn small_bundle() -> Vec<(&'static str, Page)> {
        vec![
            (
                "knowledge/dataset/drugbank.md",
                page("type: Dataset\nidentifier: DrugBank\nxref: [infores:drugbank]\n"),
            ),
            (
                "knowledge/disease/headache.md",
                page("type: Disease\nidentifier: Headache\n"),
            ),
            (
                "knowledge/molecule/aspirin.md",
                page(&format!(
                    "type: Molecule\nidentifier: Aspirin\nedges:\n  \
                     - predicate: treats\n    object: Headache\n{CITED}"
                )),
            ),
        ]
    }

    #[test]
    fn a_conformant_bundle_reports_nothing() {
        let report = check_bundle(&small_bundle());
        assert!(report.is_empty(), "{:?}", report.findings);
        assert_eq!(report.errors(), 0);
    }

    /// The profile runs the OKF layer too, so a page that breaks the *format*
    /// is reported by `okf.…` and a page that breaks the *vocabulary* by
    /// `biookf.…`. A reader can always tell which layer objected — and a base
    /// that is not in BioOKF mode can be offered the first list alone.
    #[test]
    fn both_layers_report_and_their_rule_ids_say_which_is_which() {
        let report = check(
            Some("knowledge/x.md"),
            &page("identifier: A page with no type\n"),
            &BundleIndex::unindexed(),
        );
        let prefixes: Vec<&str> = report
            .findings
            .iter()
            .map(|f| f.rule.split_once('.').expect("prefixed rule id").0)
            .collect();
        assert!(prefixes.contains(&"okf"), "{:?}", report.findings);
        assert!(prefixes.contains(&"biookf"), "{:?}", report.findings);
        // Both layers agree the page is the same page.
        for finding in &report.findings {
            assert_eq!(finding.path.as_deref(), Some("knowledge/x.md"));
        }
    }

    #[test]
    fn frontmatter_only_checking_skips_the_okf_layer_rather_than_half_running_it() {
        let doc = page("type: Sandwich\nidentifier: A sandwich\n").doc;
        let report = check_doc(None, &doc, &BundleIndex::unindexed());
        assert!(report.has(RULE_TYPE_INVALID));
        // No `okf.frontmatter.absent`: the caller already parsed it, so a
        // finding about the block would be a finding about the caller.
        assert!(report
            .findings
            .iter()
            .all(|f| f.rule.starts_with("biookf.")));
    }

    /// The bundle-scoped rules only work when the index is built from the same
    /// pages that are then checked; a lint over a subset would report every
    /// edge leaving the subset as broken.
    #[test]
    fn the_bundle_entry_point_resolves_edges_within_the_bundle_it_was_given() {
        let whole = check_bundle(&small_bundle());
        assert!(!whole.has(RULE_EDGE_OBJECT_UNRESOLVED));

        let subset = &small_bundle()[2..];
        let partial = check_bundle(subset);
        assert!(partial.has(RULE_EDGE_OBJECT_UNRESOLVED));
        // …and it is only ever a warning, so a partial lint is noisy, never
        // wrong.
        assert_eq!(partial.errors(), 0);
    }

    #[test]
    fn a_duplicate_identifier_is_reported_on_every_page_that_carries_it() {
        let mut pages = small_bundle();
        pages.push((
            "knowledge/molecule/aspirin-again.md",
            page("type: Molecule\nidentifier: Aspirin\n"),
        ));
        let report = check_bundle(&pages);
        let duplicates: Vec<_> = report.matching(RULE_IDENTIFIER_DUPLICATE).collect();
        assert_eq!(duplicates.len(), 2, "{duplicates:?}");
        let paths: Vec<_> = duplicates
            .iter()
            .filter_map(|f| f.path.as_deref())
            .collect();
        assert!(paths.contains(&"knowledge/molecule/aspirin.md"));
        assert!(paths.contains(&"knowledge/molecule/aspirin-again.md"));
    }

    /// DR-7, stated as a property of the surface: there is no `is_valid()` to
    /// call, only counts. A caller that wants strictness — a Stage 4
    /// `kb_write_page` in BioOKF mode — has to say so by reading
    /// [`Report::errors`], which makes the strictness a *producer* decision at
    /// a visible call site rather than a silent property of reading.
    #[test]
    fn the_report_offers_counts_and_never_a_verdict() {
        let report = check_bundle(&[(
            "knowledge/x.md",
            page(&format!(
                "type: Molecule\nidentifier: Ibuprofen\nedges:\n  \
                 - predicate: treats\n    object: Nothing\n{CITED}  \
                 - predicate: not_treats\n    object: Nothing\n{CITED}"
            )),
        )]);
        assert!(report.has(RULE_EDGE_CONTRADICTION));
        assert_eq!(report.errors(), 0);
        assert!(report.warnings() >= 1);
        assert_eq!(
            report.count(Severity::Error) + report.count(Severity::Warning) + report.infos(),
            report.findings.len(),
            "every finding lands on one of the three rungs and no other"
        );
    }
}

/// The BioOKF spec's own §12 worked example, linted against the BioOKF spec's
/// own §6 tables.
///
/// It is here rather than in [`super::lint`] because it is the only test that
/// runs a whole real document through both layers, and because it found
/// something: **SPEC §12 and SPEC §6.D disagree with each other.** §12 models
/// tocilizumab's adverse effect as `Molecule has_phenotype neutropenia`, while
/// §6.D gives `has_phenotype` the domain `Disease·Organism·Variant` and routes
/// drug → adverse-event through `causes` instead ("incl. … **drug→adverse-event**
/// as object=Phenotype"). The table is followed, because §6 *is* the domain/range
/// table and §12 is prose around an example — but the disagreement is pinned
/// here rather than left as a surprise, so a future reader meets it as a
/// recorded decision instead of as a mysterious warning on the spec's own page.
#[cfg(test)]
mod worked_example {
    use super::tests_support::*;
    use super::*;
    use crate::knowledge::biookf::lint::RULE_EDGE_DOMAIN;
    use crate::knowledge::okf::fixtures::TOCILIZUMAB;

    /// The nodes §12's example points at, each minimal and each typed the way
    /// §5's cheatsheet types it.
    fn referenced_nodes() -> Vec<(&'static str, Page)> {
        vec![
            typed("molecule/il6r", "Molecule", "IL6 receptor (IL6R)"),
            typed("pathway/il6", "BiologicalPathway", "IL6 signaling"),
            typed("disease/ra", "Disease", "rheumatoid arthritis"),
            typed("disease/covid", "Disease", "COVID-19"),
            typed("phenotype/neutropenia", "Phenotype", "neutropenia"),
            typed("class/il6i", "MolecularClass", "IL6 inhibitors"),
            source(
                "study/recovery",
                "Study",
                "RECOVERY trial",
                "clinicaltrials:NCT04381936",
            ),
            source(
                "dataset/drugbank",
                "Dataset",
                "DrugBank",
                "infores:drugbank",
            ),
            source(
                "dataset/drugcentral",
                "Dataset",
                "DrugCentral",
                "infores:drugcentral",
            ),
            source("dataset/sider", "Dataset", "SIDER", "infores:sider"),
            source("dataset/atc", "Dataset", "ATC", "infores:atc"),
        ]
    }

    #[test]
    fn spec_12s_worked_example_is_clean_apart_from_the_spec_6_d_disagreement() {
        let mut pages = referenced_nodes();
        pages.push((
            "knowledge/molecule/tocilizumab.md",
            Page::parse(TOCILIZUMAB).expect("the spec's own example parses"),
        ));
        let report = check_bundle(&pages);

        assert_eq!(report.errors(), 0, "{:?}", report.findings);
        let domain: Vec<_> = report.matching(RULE_EDGE_DOMAIN).collect();
        assert_eq!(domain.len(), 1, "{:?}", report.findings);
        assert!(
            domain[0].message.contains("has_phenotype"),
            "{}",
            domain[0].message
        );
        // Nothing else at all: seven edges, each with its triplet, each
        // resolving, each inside its table.
        assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    }
}

#[cfg(test)]
mod tests_support {
    use super::Page;

    fn parse(frontmatter: &str) -> Page {
        Page::parse(&format!("---\n{frontmatter}---\n\n# body\n")).expect("fixture parses")
    }

    pub fn typed(path: &'static str, node_type: &str, identifier: &str) -> (&'static str, Page) {
        (
            path,
            parse(&format!("type: {node_type}\nidentifier: {identifier}\n")),
        )
    }

    /// A source node with an external anchor, so §8.1's unanchored-source
    /// warning is not the thing under test.
    pub fn source(
        path: &'static str,
        node_type: &str,
        identifier: &str,
        xref: &str,
    ) -> (&'static str, Page) {
        (
            path,
            parse(&format!(
                "type: {node_type}\nidentifier: {identifier}\nxref: [{xref}]\n"
            )),
        )
    }
}
