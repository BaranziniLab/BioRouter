//! Resolving a `[[…]]` link to a page in *this* bundle — the half
//! [`crate::knowledge::okf::links`] deliberately does not do.
//!
//! The split is: `okf::links` reads the grammars and hands back the target
//! exactly as written; this module decides which page that target names. Two
//! modules because resolution needs the bundle and the grammar does not, and
//! because a reader that half-resolves is the harder thing to debug — the caller
//! cannot tell a link that was never written from one that was rewritten on the
//! way through.
//!
//! ## Why this module exists at all (DR-14)
//!
//! The regex `\[\[([^\]]+)\]\]` used to be written three times in this tree, each
//! followed by a *different* resolver:
//!
//! | Consumer | Resolver |
//! | --- | --- |
//! | `graph.rs` | split the `\|` alias off, strip a `knowledge/…/` prefix and a `.md` suffix, key on `slug(basename).to_lowercase()` |
//! | `macros/query.rs` | none — the raw capture, verbatim |
//! | `macros/lint.rs` | lowercase, spaces to hyphens, compare to the file stem |
//!
//! They already disagreed, and nothing failed when they did:
//! `[[knowledge/entities/x|X]]` was an edge in the graph and an *orphan* in the
//! lint, for the same page, on the same day. The three were each correct for the
//! form its author had in mind, which is exactly how the fourth grammar (BioOKF
//! inline edge sugar) came to be readable by none of them.
//!
//! The graph's resolver is the one that survived here, because it is the most
//! complete of the three and the only one with tests. Lint and query moved onto
//! it, which is a *change* for both — a piped path-style link now resolves for
//! them too — and the equivalence test at the bottom of this file is what makes
//! that a decision rather than a drift.
//!
//! ## Why [`wiki_links`] still filters markdown links out
//!
//! `okf::links::extract_links` also returns OKF §6.1 `[label](/path/to/x.md)`
//! links, which OKF calls untyped directed edges. Stage 2 widened the *graph
//! deriver* to read them (DR-2), and did it by calling `extract_links` directly
//! rather than by widening this helper — because the widening is a graph
//! decision and this helper has two other callers.
//!
//! **`macros/lint.rs` has since moved off this helper entirely**, and that is
//! the change this paragraph used to argue against. The evidence it was waiting
//! for arrived from the running app: a BioOKF base of 11 pages and 17 typed
//! edges reported all 11 pages as orphans, because a base in that profile writes
//! its relationships in `edges:` frontmatter and a `[[…]]`-only view of it is
//! empty. Lint now asks `graph::bundle_links`, so its two link-shaped rules read
//! precisely the edge set the graph draws — which is the stronger form of the
//! property this module exists for: not "the same resolver" but "the same
//! edges". Raw evidence remains an `external` graph node, not a concept page.
//! Lint separately checks whether such a target names an existing, confined
//! file under `raw/` before warning: a valid source citation is not a missing
//! concept. Missing files and unsafe paths still produce warnings.
//!
//! `macros/query.rs` now reads native Markdown citations directly, but only
//! accepts bundle page targets under `knowledge/` and rejects parent traversal.
//! That keeps `../raw/pmid-1/original.pdf` out of the user-facing citation list
//! without making this legacy helper mean two different things. Wiki-link
//! compatibility still comes through [`wiki_links`], then both forms share the
//! same page-target normalization in the query module.

use crate::knowledge::okf::links::{extract_links, LinkForm, LinkRef};
use std::collections::HashMap;

/// Every `[[…]]` link in a body, in document order — the plain/aliased legacy
/// form and BioOKF's inline edge sugar, and nothing else. See the module header
/// for why markdown links are excluded.
pub fn wiki_links(body: &str) -> Vec<LinkRef> {
    extract_links(body)
        .into_iter()
        .filter(|l| matches!(l.form, LinkForm::LegacyWiki | LinkForm::BioOkfEdgeSugar))
        .collect()
}

/// Reduce either side of a resolution — a link target as written, or a page's
/// own logical path — to the one string they are compared on.
///
/// The three reductions, in order, each of which a real link on disk depends on:
///
/// 1. **Basename.** A target may be a bare title (`Zone-2 base`) or a full
///    logical path (`knowledge/entities/zone-2-base`); the sub-agent emits both.
/// 2. **Drop `.md`.** The same target is written with and without it, sometimes
///    in the same page.
/// 3. **Slug + lowercase.** Spaces, punctuation and case are all noise here:
///    `[[Zone-2 base]]` has to find `zone-2 base.md`.
///
/// Interior separator runs are deliberately *not* collapsed (`a--b` stays
/// `a--b`): both sides go through this same function, so a collapse would buy
/// nothing and would silently merge two pages whose names differ only in
/// punctuation.
pub fn link_key(target: &str) -> String {
    let basename = target.rsplit('/').next().unwrap_or(target);
    slug(&basename.trim_end_matches(".md").to_lowercase())
}

/// The key for the *identifier* and *title* rungs of DR-3's identity ladder —
/// the whole string slugged, with no basename split.
///
/// [`link_key`] and this one differ in exactly one step, and the step matters.
/// `link_key` throws away everything before the last `/` because its input is a
/// path; an `identifier` is a name and may legitimately contain a slash
/// (`CD4/CD8 ratio`, `mg/dL`), so splitting on it would index that page under
/// `cd8-ratio` and make its own identifier fail to find it.
///
/// Both keys share [`slug`], so case, spaces and punctuation are folded exactly
/// once and identically — the property DR-14 is about, applied to a second
/// rung rather than abandoned for it.
pub fn identity_key(name: &str) -> String {
    slug(&name.to_lowercase())
}

/// Was this target written as a **path** (`knowledge/molecule/il-6.md`,
/// `il-6.md`) rather than as a **name** (`IL-6`)?
///
/// The two `[[…]]` heads look identical to a regex and are not the same
/// grammar: a bare name is the `identifier`/`title` rung of DR-3's ladder and
/// reduces through [`identity_key`], while the Obsidian path form is the
/// basename rung and reduces through [`link_key`]. The rule is exactly the
/// syntax that separates the two reductions — a `/` that `link_key` splits on,
/// or the `.md` it strips — so it lives here beside them and not beside each
/// consumer that needs to choose.
///
/// ⚠ **It orders the two rungs; it must never be used to skip one.** The
/// shapes overlap: an `identifier` may legitimately contain a slash
/// (`CD4/CD8 ratio`), so `[[CD4/CD8 ratio]]` reads as path-shaped here while
/// naming a concept, and a consumer that took this answer as final would leave
/// that link pointing at whatever the old name now resolves to. Try the rung
/// this favours first, then the other one — which is what `graph::NodeIndex`
/// already does for resolution and what [`crate::knowledge::merge`] does for
/// rewriting.
pub fn written_as_path(target: &str) -> bool {
    let target = target.trim();
    target.contains('/') || target.ends_with(".md")
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// The pages of one bundle, keyed for link resolution.
///
/// Generic over what a consumer wants back — the graph wants a node id, the lint
/// wants the logical path — so that the *keying* cannot differ between them even
/// though the values do. That was the whole bug: three keyings, three answers.
///
/// Two pages whose basenames reduce to the same key collide, and the last one
/// inserted wins. That is the behaviour the graph has always had; making it an
/// error would fail bases that exist today, and making it a multi-map is a
/// Stage 2 decision about the DR-3 identity ladder, not a seam.
#[derive(Debug, Clone, Default)]
pub struct LinkIndex<T> {
    by_key: HashMap<String, T>,
}

impl<T> LinkIndex<T> {
    /// Build from `(page identity as written, value)` pairs — a logical path
    /// (`knowledge/entities/hrv.md`) or a bare page name; both reduce through
    /// [`link_key`].
    pub fn from_pages(pages: impl IntoIterator<Item = (String, T)>) -> Self {
        Self {
            by_key: pages
                .into_iter()
                .map(|(identity, value)| (link_key(&identity), value))
                .collect(),
        }
    }

    /// The page a link target names, or `None` for a dangling link.
    ///
    /// This is the *basename* rung of DR-3's ladder — the bottom one, and the
    /// only one that existed before Stage 2. The deriver stacks the identifier
    /// and title rungs above it (`graph::NodeIndex`); lint and query stay on
    /// this rung alone, because their question is "does a file by this name
    /// exist?" and not "which concept does this name?".
    pub fn resolve(&self, target: &str) -> Option<&T> {
        self.by_key.get(&link_key(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::Graph;
    use std::path::Path;

    #[test]
    fn a_bare_title_and_its_logical_path_reduce_to_the_same_key() {
        for written in [
            "Zone-2 base",
            "zone-2 base",
            "knowledge/concepts/zone-2 base",
            "knowledge/concepts/zone-2 base.md",
            "zone-2-base.md",
        ] {
            assert_eq!(link_key(written), "zone-2-base", "for {written}");
        }
    }

    #[test]
    fn wiki_links_reads_both_bracket_forms_and_leaves_markdown_links_alone() {
        let links = wiki_links("[[A]] and [b](/knowledge/entities/b.md) and [[treats:: C]]");
        let targets: Vec<&str> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(
            targets,
            vec!["A", "C"],
            "this compatibility helper deliberately excludes markdown links; \
             graph, lint, and native query citations read them at their own boundaries"
        );
    }

    #[test]
    fn a_name_containing_a_slash_keeps_it_and_a_path_does_not() {
        // The whole difference between the two keys, and the bug it prevents:
        // an `identifier` may legitimately contain a slash, so reducing it the
        // way a path is reduced would index `CD4/CD8 ratio` under `cd8-ratio`
        // and make the page unfindable by its own name.
        assert_eq!(identity_key("CD4/CD8 ratio"), "cd4-cd8-ratio");
        assert_eq!(link_key("CD4/CD8 ratio"), "cd8-ratio");
        // On a name with no slash the two agree, which is why a legacy base —
        // where every target is a bare title or a path — resolves identically.
        assert_eq!(identity_key("Zone-2 base"), link_key("Zone-2 base"));
    }

    #[test]
    fn the_alias_is_not_part_of_the_target() {
        // The disagreement DR-14 names: with the alias attached, the slug of
        // the whole payload matched no page, so the lint called the target page
        // an orphan while the graph drew an edge to it.
        let links = wiki_links("[[knowledge/entities/wanjun-gu.md|Wanjun Gu]]");
        assert_eq!(link_key(&links[0].target), "wanjun-gu");
    }

    #[test]
    fn a_path_shaped_target_is_told_from_a_name_by_the_syntax_link_key_strips() {
        for path in [
            "knowledge/concepts/zone-2 base",
            "knowledge/concepts/zone-2 base.md",
            "zone-2-base.md",
            // The overlap the predicate cannot resolve on its own, and the
            // reason its doc forbids using it to skip a rung: this is an
            // `identifier`, and it reads as path-shaped.
            "CD4/CD8 ratio",
        ] {
            assert!(written_as_path(path), "for {path}");
        }
        for name in ["Zone-2 base", "IL-6", "COVID-19", "mg dL"] {
            assert!(!written_as_path(name), "for {name}");
            // The whole claim: on a name the two reductions agree, so ordering
            // the rungs costs nothing; on a path they do not, which is what
            // makes choosing between them necessary at all.
            assert_eq!(identity_key(name), link_key(name), "for {name}");
        }
    }

    #[test]
    fn an_index_resolves_every_spelling_of_the_same_page() {
        let index = LinkIndex::from_pages([(
            "knowledge/concepts/zone-2 base.md".to_string(),
            "concepts:zone-2 base",
        )]);
        for target in [
            "Zone-2 base",
            "zone-2 base",
            "knowledge/concepts/zone-2 base",
            "knowledge/concepts/zone-2 base.md",
        ] {
            assert_eq!(
                index.resolve(target).copied(),
                Some("concepts:zone-2 base"),
                "for {target}"
            );
        }
        assert_eq!(index.resolve("Nonexistent Page"), None);
    }

    /// The structural half of DR-14: the three consumers must not carry a
    /// bracket regex of their own again.
    ///
    /// The equivalence test below catches a *divergence*; this catches the way
    /// divergence happens — someone writing `Regex::new(r"\[\[…")` beside a new
    /// feature in one of the three files, which reads perfectly plausible and
    /// re-forks the parser. The grammar has exactly one reader
    /// ([`crate::knowledge::okf::links`]) and this file has the only resolver.
    #[test]
    fn no_consumer_re_spells_the_bracket_regex() {
        // Assembled at runtime so this test's own text cannot satisfy it.
        let needle = concat!("\\[", "\\[");
        for (name, src) in [
            ("graph.rs", include_str!("graph.rs")),
            ("macros/query.rs", include_str!("macros/query.rs")),
            ("macros/lint.rs", include_str!("macros/lint.rs")),
        ] {
            assert!(
                !src.contains(needle),
                "{name} spells the wiki-link pattern itself again; call \
                 knowledge::links instead — three copies with three resolvers is \
                 what DR-14 is about"
            );
        }
    }

    // -----------------------------------------------------------------------
    // The equivalence test (DR-14's gate)
    // -----------------------------------------------------------------------

    /// The link shapes the sub-agent actually emits, one *page* each:
    /// `(page stem, the single link that must reach that page)`.
    ///
    /// One page per shape is the load-bearing part, and it is the fix for a test
    /// that could not fail. The first corpus pointed all four shapes at one page
    /// and asked whether that page was an orphan — a question any one of the
    /// four satisfies. Mutating the lint's inbound resolution back to its old
    /// stem comparison left the test green, because the plain link alone kept
    /// the page linked while the piped and path-style handling (the
    /// disagreement DR-14 is *about*) was broken. Here each page's only inbound
    /// link is written in one shape, so breaking that shape orphans that page
    /// and no other link covers for it.
    ///
    /// The stems are hyphenated deliberately: `[[Plain target]]` resolves under
    /// the *old* per-consumer resolvers too, so the plain row is the control
    /// that stays green while the other three go red. A shape every resolver
    /// ever written got right cannot tell two resolvers apart.
    const SHAPES: [(&str, &str); 4] = [
        ("plain-target", "[[Plain target]]"),
        ("piped-target", "[[piped-target|the alias]]"),
        ("path-target", "[[knowledge/concepts/path-target]]"),
        ("suffix-target", "[[knowledge/concepts/suffix-target.md]]"),
    ];

    /// A link to a page that does not exist. All three consumers must agree it
    /// resolves to nothing — the other half of "which links resolve?", and the
    /// half a resolver that matches too eagerly gets wrong.
    const DANGLING: &str = "Nonexistent Page";

    /// The node id of the one page every link above is written in.
    const SOURCE_NODE: &str = "sources:paper";

    fn concept_path(stem: &str) -> String {
        format!("knowledge/concepts/{stem}.md")
    }

    fn concept_node(stem: &str) -> String {
        format!("concepts:{stem}")
    }

    /// One line per shape, in `SHAPES` order, then the dangling one — the query
    /// assertion reads the citation list back positionally against it.
    fn source_body() -> String {
        let mut body = String::new();
        for (stem, link) in SHAPES {
            body.push_str(&format!("The {stem} page: {link}.\n"));
        }
        body.push_str(&format!("And a link to nothing: [[{DANGLING}]].\n"));
        body
    }

    fn write_corpus(kb: &Path) {
        use crate::knowledge::{page_fixtures::valid_page, store::write_page};

        // A *source* page, because the lint's missing-concept check looks only
        // at those — write the dangling link anywhere else and that half of the
        // report is empty for a reason that has nothing to do with resolution.
        write_page(
            kb,
            "knowledge/sources/paper.md",
            &valid_page("source", "Paper", &source_body()),
            "add paper",
            None,
        )
        .unwrap();
        for (stem, _) in SHAPES {
            write_page(
                kb,
                &concept_path(stem),
                &valid_page("concept", stem, "A target page."),
                "add target",
                None,
            )
            .unwrap();
        }
    }

    /// Exactly one edge per shape, and no node invented for the dangling target.
    fn assert_graph_agrees(g: &Graph) {
        for (stem, link) in SHAPES {
            let to = concept_node(stem);
            let drawn = g
                .edges
                .iter()
                .filter(|e| e.from == SOURCE_NODE && e.to == to)
                .count();
            assert_eq!(
                drawn, 1,
                "{link} is the only link to {to}, so {drawn} edges means the \
                 deriver stopped resolving that shape; edges={:?}",
                g.edges
            );
        }
        assert!(
            g.nodes.iter().all(|n| n.label != DANGLING),
            "a dangling link must not invent a node; nodes={:?}",
            g.nodes
        );
    }

    /// The lint must see the same inbound link the graph drew an edge for, and
    /// must report as missing exactly the target the graph refused to resolve.
    fn assert_lint_agrees(kb: &Path) {
        let report = crate::knowledge::macros::lint::scan(kb).unwrap();
        for (stem, link) in SHAPES {
            let path = concept_path(stem);
            assert!(
                !report.orphans.contains(&path),
                "{path} has exactly one inbound link, {link} — an orphan here is \
                 the lint contradicting the edge the graph just drew, and sends \
                 the user to fix a page that is fine; orphans={:?}",
                report.orphans
            );
        }
        assert_eq!(
            report.missing_concept_pages,
            vec![DANGLING.to_string()],
            "the dangling target and nothing else: a resolvable shape listed \
             here is the missing-concept check resolving differently from the \
             inbound-link check fifty lines above it"
        );
    }

    /// The citation extractor has no bundle to resolve against, so what is
    /// compared is the *target it hands back*: run those through the shared
    /// index and every answer must match the graph's.
    fn assert_query_agrees(g: &Graph) {
        let cited = crate::knowledge::macros::query::extract_wiki_links_from_text(&source_body());
        let index: LinkIndex<String> = LinkIndex::from_pages(
            g.nodes
                .iter()
                .map(|n| (n.path.clone(), n.id.clone()))
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            cited.len(),
            SHAPES.len() + 1,
            "one citation per link, in document order, plus the dangling one: {cited:?}"
        );
        for (i, (stem, link)) in SHAPES.iter().enumerate() {
            let want = concept_node(stem);
            assert_eq!(
                index.resolve(&cited[i]),
                Some(&want),
                "query handed back {:?} for {link}; a citation is prose shown to \
                 the user, so a target the shared index rejects is a citation \
                 pointing nowhere",
                cited[i]
            );
        }
        assert_eq!(
            cited[SHAPES.len()],
            DANGLING,
            "the dangling target must survive extraction verbatim: {cited:?}"
        );
        assert!(
            index.resolve(DANGLING).is_none(),
            "the dangling target must resolve nowhere"
        );
        assert!(
            !cited.iter().any(|c| c.contains('|')),
            "an alias reached the citation list verbatim — the old query \
             resolver, which had none: {cited:?}"
        );
    }

    /// One corpus, all three consumers, one question: *which links resolve?*
    ///
    /// This is the test that could not be written while the three regexes and
    /// their three resolvers were separate — and the reason it is worth having
    /// even now that they share a path is that the sharing is not structurally
    /// enforced. Nothing stops a future edit re-resolving in one consumer; this
    /// test is what notices, because that consumer would then disagree with the
    /// other two about one of the shapes in [`SHAPES`].
    ///
    /// It is checked by mutation, not by being green: break any one of the four
    /// resolution sites — the graph's edge resolve, the lint's inbound resolve,
    /// the lint's missing-concept resolve, or the query extractor's target — and
    /// exactly the assertion for that consumer fires.
    #[tokio::test]
    async fn graph_lint_and_query_agree_on_what_resolves() {
        use crate::knowledge::{graph, service::KnowledgeService};

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        write_corpus(&kb);

        let g = graph::derive(&kb).unwrap();
        assert_graph_agrees(&g);
        assert_lint_agrees(&kb);
        assert_query_agrees(&g);
    }
}
