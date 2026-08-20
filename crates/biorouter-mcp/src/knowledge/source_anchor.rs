//! Recognising a **source page** — the page in the `knowledge/` tree that
//! stands for one ingested document under `raw/<id>/` — across every layout this
//! build can produce.
//!
//! # Why this is one module and not two answers
//!
//! Two surfaces ask this question and they must not disagree about it:
//!
//! * [`graph::apply_source_credibility`] attaches a source's credibility tier
//!   and its `retracted` flag to the node that stands for it, so a base citing a
//!   retracted paper draws differently from one that does not.
//! * [`macros::lint`]'s `stale_sources` and `missing_concept_pages` rules ask
//!   which raw source a page speaks for, and which pages are source pages at
//!   all.
//!
//! Both used to answer it from the literal string `knowledge/sources/`. That is
//! the **pre-OKF** layout, and no base created since the format chooser landed
//! uses it — so on every current base credibility silently never attached, every
//! raw source over 90 days old was reported stale however heavily cited, and
//! `missing_concept_pages` was unconditionally empty. Each of those reads
//! exactly like a healthy base, which is why all three survived a full test
//! suite.
//!
//! The first repair replaced the path with BioOKF's `raw_source` anchor and kept
//! the path as a fallback. That fixed BioOKF and left **plain OKF — the default
//! for a new base — as broken as before**, because an OKF base has neither
//! signal:
//!
//! * `raw_source` is a §7.1 **BioOKF** key. The only tool that emits it,
//!   `write_concept_spec`, is offered only `if format.is_some_and(KbFormat::is_biookf)`,
//!   and the only automatic writer, `macros::ingest::materialize_source_node`,
//!   is gated identically. No OKF page carries it, ever.
//! * OKF's source directory is the **SINGULAR** `knowledge/source/` — see
//!   `schema_okf.md`'s layout section — not the plural the pre-OKF schema used.
//!
//! # The signals, and which question each one settles
//!
//! | signal | layout | says |
//! |---|---|---|
//! | `raw_source: [raw/<id>/…]` | BioOKF §7.1 | *is* that source's page |
//! | `resource: raw/<id>/…` | OKF §4.1 — the resource the page is about | *is* that source's page |
//! | `sources: [{resource: raw/<id>/…}]` | OKF §5.1 provenance | names that source |
//! | `type` naming a source | OKF (`Source`) / BioOKF §8.1 (`Publication`, `Study`, `Dataset`, `Agent`) | is a source page |
//! | `kind: source` | the pre-OKF schema's own declaration | is a source page |
//! | `knowledge/source/` or `knowledge/sources/` | OKF layout / pre-OKF layout | is a source page |
//!
//! ⚠ **`sources[]` is read only on a page the other signals already call a
//! source page.** OKF's ingest workflow tells the model to record the source in
//! `sources` on *every concept page it touches* (`schema_okf.md` step 4), so a
//! bare "cites `raw/<id>/…`" test would call every concept page in the bundle a
//! source page. `missing_concept_pages` would then report every unresolved link
//! anywhere in the base — a louder failure than the empty list it replaces — and
//! a retracted paper's flag would land on an arbitrary concept. `resource:` is
//! different, and that is why it counts on its own: §4.1 makes it the resource
//! the page *is about*, so a page pointing it at `raw/<id>/` is claiming to be
//! that source's page.
//!
//! [`graph::apply_source_credibility`]: super::graph
//! [`macros::lint`]: super::macros::lint

use super::biookf;
use super::okf::ConceptDoc;

/// Either directory a source page may live in, and the plural is not a typo.
///
/// `knowledge/sources/` is what bases created before the format chooser
/// scaffolded; `knowledge/source/` is what OKF scaffolds. Reading only the
/// plural — which is what both callers used to do — is exactly how a rule goes
/// blind to every base the current build creates while still passing every test
/// written against a fixture from the old one.
pub(crate) fn is_source_dir(path: &str) -> bool {
    path.starts_with("knowledge/sources/") || path.starts_with("knowledge/source/")
}

/// Whether a page's declared type says it is a source.
///
/// Three vocabularies, because a bundle may have been written under any of them:
///
/// * **BioOKF §8.1** closes the set at `Publication`, `Study`, `Dataset` and
///   `Agent` — the only node types that may bear a `primary_source`. Asked
///   through `NodeType::is_source` rather than restating the four, for the same
///   reason `biookf::lint::check_source_unanchored` asks it: a fifth source type
///   must not become true in one file and false in the other.
/// * **OKF** has an open vocabulary, so there is no set to close — but its
///   layout names `source/` as a starter directory, which makes the type behind
///   it `Source`. Compared case-insensitively because OKF never validates
///   `type`, and a model writes `source` as readily as `Source`.
/// * **The pre-OKF schema** declared it outright, in a separate `kind:` key
///   whose values are `entity | concept | source | note | hub`. That key is not
///   in OKF's model, so it arrives through the preserved-unknown-keys mapping.
pub(crate) fn is_source_type(doc: &ConceptDoc) -> bool {
    if biookf::NodeType::parse(&doc.r#type).is_some_and(biookf::NodeType::is_source) {
        return true;
    }
    if doc.r#type.eq_ignore_ascii_case("source") {
        return true;
    }
    doc.extra
        .get(serde_yaml::Value::String("kind".into()))
        .and_then(|v| v.as_str())
        .is_some_and(|kind| kind.eq_ignore_ascii_case("source"))
}

/// The raw ids a page claims to **be** the page for: BioOKF's `raw_source`
/// anchor and OKF's own `resource`.
///
/// Both are parsed by [`biookf::lint::raw_source_id`] rather than by three
/// copies of the same split: its job is to confine a model-written frontmatter
/// string to a single path segment — `raw_source: ../../../.ssh/id_rsa` must not
/// become a path join — and a second copy of that is a second thing to get
/// wrong.
pub(crate) fn anchored_raw_ids(doc: &ConceptDoc) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in biookf::lint::raw_source(doc) {
        if let Some(id) = biookf::lint::raw_source_id(&entry) {
            out.push(id.to_string());
        }
    }
    if let Some(id) = doc
        .resource
        .as_deref()
        .and_then(biookf::lint::raw_source_id)
    {
        out.push(id.to_string());
    }
    out
}

/// Whether this page is a source page at all.
///
/// The union of every signal but `sources[]`, which cannot be read here — see
/// the module header for why a page that merely *cites* a raw source is not that
/// source's page.
pub(crate) fn is_source_page(path: &str, doc: &ConceptDoc) -> bool {
    is_source_dir(path) || is_source_type(doc) || !anchored_raw_ids(doc).is_empty()
}

/// Every raw source this page stands for, de-duplicated and in the order the
/// signals are listed in the module header.
///
/// Empty for a page that is not a source page, and for a source page that names
/// no raw source at all — a hand-written `knowledge/source/notes.md` is a source
/// page with nothing under `raw/` behind it, and saying so is more useful than
/// guessing an id from its filename.
pub(crate) fn raw_ids_stood_for(path: &str, doc: &ConceptDoc) -> Vec<String> {
    let mut out = anchored_raw_ids(doc);
    // The §5.1 list, admitted only because the page already passed one of the
    // other tests. On an OKF base this is often the *only* place the raw id
    // appears, because §5.1 provenance is how OKF states it and `raw_source`
    // does not exist there.
    if is_source_dir(path) || is_source_type(doc) {
        for source in &doc.sources {
            if let Some(id) = source
                .resource
                .as_deref()
                .and_then(biookf::lint::raw_source_id)
            {
                out.push(id.to_string());
            }
        }
    }
    // De-duplicated in place rather than through a set, because the ORDER is
    // part of the contract: `pages_for` feeds a report the user diffs against
    // the last one, and a `HashSet` would reorder it run to run. `dedup` alone
    // would not do — it only collapses ADJACENT equals, and the same id can
    // appear in `raw_source` and again several entries into `sources[]`.
    let mut seen: Vec<String> = Vec::new();
    out.retain(|id| {
        let first = !seen.iter().any(|s| s == id);
        if first {
            seen.push(id.clone());
        }
        first
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf;

    fn doc(frontmatter: &str) -> ConceptDoc {
        okf::Page::parse(&format!("---\n{frontmatter}---\n\n# x\n"))
            .expect("fixture parses")
            .doc
    }

    /// The gap the second pass closed: a **plain OKF** base, which is what
    /// `create_base` produces by default. It has no `raw_source` key anywhere —
    /// the only two writers of that key are gated on `KbFormat::is_biookf` — and
    /// its source directory is the singular one, so the pre-OKF plural path and
    /// the BioOKF anchor both miss it completely.
    #[test]
    fn an_okf_source_page_is_recognised_by_its_own_provenance() {
        let page = doc("type: Source\nidentifier: Chen 2020\n\
             sources:\n  - id: chen-2020\n    resource: raw/chen-2020/original.pdf\n");
        let path = "knowledge/source/chen-2020.md";
        assert!(is_source_page(path, &page), "OKF's singular directory");
        assert_eq!(
            raw_ids_stood_for(path, &page),
            vec!["chen-2020".to_string()],
            "OKF states provenance as `sources[].resource`, and on an OKF base \
             that is the only place the raw id appears"
        );
    }

    /// ⚠ The restriction the module header argues for, as an assertion rather
    /// than a comment. OKF's ingest workflow tells the model to record the
    /// source in `sources` on every concept page it touches, so if `sources[]`
    /// counted on its own, every concept page in the bundle would be a source
    /// page — and `missing_concept_pages` would report every unresolved link
    /// anywhere in the base.
    #[test]
    fn a_concept_page_that_merely_cites_a_source_is_not_that_sources_page() {
        let page = doc("type: Disease\nidentifier: COVID-19\n\
             sources:\n  - id: chen-2020\n    resource: raw/chen-2020/original.pdf\n");
        let path = "knowledge/disease/covid-19.md";
        assert!(!is_source_page(path, &page));
        assert!(raw_ids_stood_for(path, &page).is_empty());
    }

    /// BioOKF's anchor still settles both questions on its own, wherever the
    /// page lives — `materialize_source_node` writes it to
    /// `knowledge/<lowercased type>/<id>.md`, which is neither directory above.
    #[test]
    fn a_biookf_anchor_stands_alone_in_a_typed_directory() {
        let page = doc(
            "type: Publication\nidentifier: Chen 2020\nraw_source: [raw/chen-2020/source.md]\n",
        );
        let path = "knowledge/publication/chen-2020.md";
        assert!(is_source_page(path, &page));
        assert_eq!(
            raw_ids_stood_for(path, &page),
            vec!["chen-2020".to_string()]
        );
    }

    /// The pre-OKF layout, which is the only one the old code could see. It
    /// keeps working, and it has to: those bases exist on disk and are exactly
    /// the ones carrying no anchor to read.
    #[test]
    fn the_pre_okf_layout_and_its_kind_key_are_both_still_source_pages() {
        let by_path = doc("title: Chen 2020\n");
        assert!(is_source_page("knowledge/sources/chen-2020.md", &by_path));
        let by_kind = doc("kind: source\ntitle: Chen 2020\n");
        assert!(is_source_page("knowledge/notes/chen-2020.md", &by_kind));
    }

    /// One page may name the same raw source twice — `materialize_source_node`
    /// writes `raw_source`, and a later edit can add the same document to
    /// `sources[]`. A path counted twice would make one inbound link look like
    /// two to `stale_sources`.
    #[test]
    fn a_source_named_in_both_grammars_is_returned_once() {
        let page = doc("type: Publication\nidentifier: Chen 2020\n\
             raw_source: [raw/chen-2020/source.md]\n\
             sources:\n  - id: chen-2020\n    resource: raw/chen-2020/original.pdf\n");
        assert_eq!(
            raw_ids_stood_for("knowledge/publication/chen-2020.md", &page),
            vec!["chen-2020".to_string()],
        );
    }

    /// The confinement `raw_source_id` exists for, asserted here because this
    /// module is now the only caller some of these spellings pass through: a
    /// frontmatter string is model-written, and none of it may become a path
    /// join.
    #[test]
    fn a_traversing_resource_names_no_raw_source() {
        for hostile in [
            "../../../.ssh/id_rsa",
            "raw/../../etc/passwd",
            "/etc/passwd",
        ] {
            let page = doc(&format!("type: Source\nresource: {hostile}\n"));
            assert!(
                raw_ids_stood_for("knowledge/source/x.md", &page).is_empty(),
                "{hostile} was accepted as a raw source id"
            );
        }
    }
}
