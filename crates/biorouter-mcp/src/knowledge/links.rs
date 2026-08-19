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
//! ## What is deliberately not resolved yet
//!
//! **Markdown links are not edges.** `okf::links::extract_links` also returns
//! OKF §6.1 `[label](/path/to/x.md)` links, which OKF calls untyped directed
//! edges — but nothing in this tree has ever derived an edge from one, and
//! turning them on is a graph change, not a seam. [`wiki_links`] filters to the
//! two `[[…]]` forms, which is precisely the set the three regexes matched.
//! Stage 2 widens it, with the graph tests that a widening needs.

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
    /// Stage 2 makes a dangling link a *recorded* fact rather than a silent
    /// drop; until then every caller drops it, as all three did before.
    pub fn resolve(&self, target: &str) -> Option<&T> {
        self.by_key.get(&link_key(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "a markdown link is not an edge until Stage 2 says so"
        );
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

    /// One corpus, all three consumers, one question: *which links resolve?*
    ///
    /// This is the test that could not be written while the three regexes and
    /// their three resolvers were separate — and the reason it is worth having
    /// even now that they share a path is that the sharing is not structurally
    /// enforced. Nothing stops a future edit re-spelling the regex in one macro;
    /// this test is what notices, because that macro would then disagree with
    /// the graph about the piped, path-style link below.
    ///
    /// The corpus carries one of each form the sub-agent actually emits: a plain
    /// link, a piped alias link, a path-style link with and without `.md`, and a
    /// link to a page that does not exist.
    #[tokio::test]
    async fn graph_lint_and_query_agree_on_what_resolves() {
        use crate::knowledge::{graph, macros, service::KnowledgeService, store::write_page};

        const BODY: &str = "\
Plain [[Zone-2 base]].
Piped [[knowledge/concepts/zone-2 base|the base]].
Path-style [[knowledge/concepts/zone-2 base.md]].
Dangling [[Nonexistent Page]].";

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        // A *source* page, so the lint's missing-concept check (which only
        // looks at source pages) sees the dangling link.
        write_page(
            &kb,
            "knowledge/sources/paper.md",
            &format!("---\ntitle: Paper\nkind: source\n---\n\n{BODY}"),
            "add paper",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/concepts/zone-2 base.md",
            "---\ntitle: Zone-2 base\nkind: concept\n---\n\nThe base.",
            "add z2",
            None,
        )
        .unwrap();

        // 1. The graph: three edges into the concept, none for the dangling one.
        let g = graph::derive(&kb).unwrap();
        let into_concept = g
            .edges
            .iter()
            .filter(|e| e.from == "sources:paper" && e.to == "concepts:zone-2 base")
            .count();
        assert_eq!(
            into_concept, 3,
            "all three resolvable forms must be edges, got {:?}",
            g.edges
        );
        assert!(
            g.nodes.iter().all(|n| n.label != "Nonexistent Page"),
            "a dangling link must not invent a node"
        );

        // 2. The lint: the concept has an inbound link (so it is not an orphan),
        //    and the dangling target is the one missing page reported.
        let report = macros::lint::scan(&kb).unwrap();
        assert!(
            !report
                .orphans
                .contains(&"knowledge/concepts/zone-2 base.md".to_string()),
            "the piped path-style link must count as inbound; orphans={:?}",
            report.orphans
        );
        assert_eq!(
            report.missing_concept_pages,
            vec!["Nonexistent Page".to_string()],
            "exactly the dangling target, and no resolvable one"
        );

        // 3. The query citation extractor, over the same text. It has no bundle
        //    to resolve against, so what is compared is the *target it hands
        //    back*: run those through the same index and the answer must match.
        let cited = macros::query::extract_wiki_links_from_text(BODY);
        let index = LinkIndex::from_pages(
            g.nodes
                .iter()
                .map(|n| (n.path.clone(), n.id.clone()))
                .collect::<Vec<_>>(),
        );
        let resolved: Vec<Option<&String>> =
            cited.iter().map(|c| index.resolve(c)).collect::<Vec<_>>();
        assert_eq!(
            resolved
                .iter()
                .filter(|r| r.map(String::as_str) == Some("concepts:zone-2 base"))
                .count(),
            3,
            "query handed back targets the shared index cannot resolve: {cited:?}"
        );
        assert!(
            cited.contains(&"Nonexistent Page".to_string())
                && index.resolve("Nonexistent Page").is_none(),
            "the dangling target must survive extraction and resolve nowhere: {cited:?}"
        );
        assert!(
            !cited.iter().any(|c| c.contains('|')),
            "an alias reached the citation list verbatim — the old query resolver: {cited:?}"
        );
    }
}
