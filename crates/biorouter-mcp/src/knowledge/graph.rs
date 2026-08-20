//! Deriving the knowledge graph from the pages on disk.
//!
//! ## Three grammars in, one edge out (DR-2, DR-25)
//!
//! A relationship can be written four ways, and this module reads all four:
//! BioOKF §6's `edges:` frontmatter array (the typed, attributed, primary one),
//! BioOKF §4.1's inline `[[predicate:: Object]]` sugar, OKF §6.1's markdown
//! links, and the legacy bracket links every base on disk is written in.
//! [`crate::knowledge::okf::links`] is the only reader of the body grammars and
//! [`crate::knowledge::links`] the only resolver, so this file holds no pattern
//! of its own — DR-14, pinned by a test that greps this source.
//!
//! Reading four grammars means a page can assert the same relationship twice —
//! BioOKF §4 says of the two link forms that "Only `edges:` entries are part of
//! the graph", and permits prose to restate one. [`dedupe`] reduces to a set
//! keyed on `(from, to, predicate)` and lets a typed entry *absorb* an untyped
//! link to the same target. Without it a BioOKF page renders every relationship
//! twice: once typed, once grey.
//!
//! ## What a legacy base derives, and why it is unchanged
//!
//! This is the property that governs every decision here: a base of untyped
//! pages carrying only `[[…]]` links must derive **exactly** the graph it
//! derived before Stage 2, modulo the new fields being absent. Two consequences
//! that read as arbitrary until that sentence is in mind:
//!
//! 1. **A dangling `[[…]]` link is still dropped.** Every other grammar records
//!    a dangling target as an `external` node — OKF §11 makes a broken link a
//!    thing consumers MUST tolerate, and a target nobody has written a page for
//!    is the curation queue, not an error. But a legacy base is `[[…]]`-only, so
//!    recording those would change its node set, and `links.rs`'s equivalence
//!    test says so in as many words: *"a dangling link must not invent a node"*.
//! 2. **Bracket links are read from the whole file, markdown links from the body
//!    alone.** The pre-Stage-2 deriver scanned the entire file text, frontmatter
//!    included, so a bracket link written inside a `description:` produced an
//!    edge; narrowing that to the body would silently delete edges from bases on
//!    disk. Markdown links get the narrower treatment because they are *newly*
//!    read, and YAML is full of things that are nearly markdown links —
//!    `sources[].resource` paths, citation lists — none of which is an edge.

use crate::knowledge::{
    biookf::NEGATION_PREFIX,
    links::{self, LinkIndex},
    okf::{self, ConceptDoc, Edge as OkfEdge, LinkForm, LinkRef},
    raw,
    store::{self, PageRef},
    types::{Graph, GraphEdge, GraphNode, PageKind, QuantitativeValue},
};
use anyhow::Result;
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub fn derive(kb_root: &Path) -> Result<Graph> {
    // Scaffold pages are dropped here rather than by the loader, because
    // [`bundle_links`] wants them: they are not graph nodes, but they *are* link
    // sources. As nodes they would be giant hubs linked to everything — see
    // [`is_scaffold_page`] — so a link pointing at one resolves to nothing.
    let pages: Vec<LoadedPage> = load_pages(kb_root)?
        .into_iter()
        .filter(|p| !is_scaffold_page(&p.page.path))
        .collect();
    // One date for the whole derivation, so two nodes with the same
    // `stale_after` can never disagree because the clock ticked between them.
    let today = chrono::Utc::now().date_naive();
    let mut nodes: Vec<GraphNode> = pages.iter().map(|p| node_for(p, today)).collect();
    apply_source_credibility(kb_root, &pages, &mut nodes)?;

    let index = NodeIndex::build(&pages);
    let mut collector = EdgeCollector::default();
    for page in &pages {
        collector.collect_page(page, &index);
    }
    // Externals last: a real page always outranks a placeholder for the same
    // concept, and appending keeps every existing node's position unchanged.
    nodes.extend(collector.external.into_values());
    // After `dedupe`, never before: DR-25 collapses a relationship written in
    // two grammars into one edge, and a degree counted over the raw list would
    // size a hub by how many ways its page happened to spell its links.
    let edges = dedupe(collector.edges);
    apply_degrees(&mut nodes, &edges);
    Ok(Graph { nodes, edges })
}

// ── The link view of a bundle (what lint asks) ──────────────────────────────

/// A bundle's links, keyed by logical path rather than by node id.
#[derive(Debug, Clone, Default)]
pub struct BundleLinks {
    /// Logical path → the logical paths of the pages that link *to* it. Every
    /// page that can be a node has an entry, empty when nothing links to it —
    /// so a reader distinguishes "nothing links here" from "no such page".
    pub inbound: HashMap<String, HashSet<String>>,
    /// `(source page's logical path, target exactly as written)` for every link
    /// that named no page in this bundle.
    pub unresolved: Vec<(String, String)>,
}

/// Every inbound link in a bundle, over all four grammars.
///
/// This exists because lint used to answer "does anything link to this page?"
/// from `okf::links`' legacy bracket form alone, and a typed base states its
/// relationships in BioOKF §6's `edges:` frontmatter array instead. Measured in
/// the running app: a BioOKF base of 11 pages and 17 typed edges showed *17
/// links* in the subject band and reported **all 11 pages as orphans** — lint
/// telling the user to fix every page in a base that was fine, and one grammar
/// away from the graph drawn beside it. The same blindness reached
/// `missing_concept_pages`.
///
/// So this is deliberately **not** another reader. It is the deriver's own
/// [`EdgeCollector`], over the same [`load_pages`], resolving through the same
/// DR-3 [`NodeIndex`] ladder, projected from node ids back onto logical paths.
/// A `derive` that starts reading a fifth grammar changes this answer with it.
///
/// Two differences from [`derive`], each demanded by the question rather than
/// chosen:
///
/// 1. **Scaffold pages are link sources.** They are not nodes, but the orphan
///    rule's own advice is "link it from a hub page", and `knowledge/index.md`
///    is that page. They are not inbound *keys*, since they are not pages the
///    rule can fire on.
/// 2. **An unresolved target is reported whether or not a placeholder was
///    drawn.** A dangling legacy `[[…]]` link invents no `external` node — that
///    is what keeps a legacy base's node set unchanged, see the module header —
///    and it is exactly what the missing-concept rule is looking for.
///
/// Re-derived rather than read from `graph-cache.json` on purpose: a cache this
/// build cannot read comes back as `Ok(None)` (DR-13), and lint has no third
/// answer to give — a hygiene report that silently emptied itself because a
/// cache was stale would be the misleading-lint bug over again.
pub fn bundle_links(kb_root: &Path) -> Result<BundleLinks> {
    let pages = load_pages(kb_root)?;
    let index = NodeIndex::build(&pages);
    let mut collector = EdgeCollector::default();
    for page in &pages {
        collector.collect_page(page, &index);
    }
    let path_of: HashMap<&str, &str> = pages
        .iter()
        .map(|p| (p.node_id.as_str(), p.page.path.as_str()))
        .collect();
    // Seeded with every page that can be a node, so a page nothing links to is
    // an empty set here and not a missing key.
    let mut inbound: HashMap<String, HashSet<String>> = pages
        .iter()
        .filter(|p| !is_scaffold_page(&p.page.path))
        .map(|p| (p.page.path.clone(), HashSet::new()))
        .collect();
    // No `dedupe` first: DR-25 collapses two spellings of one relationship, and
    // this is already a set of *pages* — the pair survives either way.
    for e in &collector.edges {
        if let (Some(from), Some(to)) = (path_of.get(e.from.as_str()), path_of.get(e.to.as_str())) {
            inbound
                .entry((*to).to_string())
                .or_default()
                .insert((*from).to_string());
        }
    }
    let unresolved = collector
        .unresolved
        .into_iter()
        .filter_map(|(from, target)| Some(((*path_of.get(from.as_str())?).to_string(), target)))
        .collect();
    Ok(BundleLinks {
        inbound,
        unresolved,
    })
}

/// UI spec §2.1's `degree`: incident edges per node, counted once per edge at
/// each end.
///
/// A self-loop cannot inflate a node here because `classify_target` drops one
/// before it is ever collected. A node with no edges gets `Some(0)` and not
/// `None` — "the producer counted, and the answer was zero" is a different
/// statement from "the producer did not supply this", and §5.6 floors `deg(n)`
/// at whichever it receives.
fn apply_degrees(nodes: &mut [GraphNode], edges: &[GraphEdge]) {
    let mut degree: HashMap<&str, u32> = HashMap::new();
    for e in edges {
        *degree.entry(e.from.as_str()).or_default() += 1;
        *degree.entry(e.to.as_str()).or_default() += 1;
    }
    for n in nodes.iter_mut() {
        n.degree = Some(degree.get(n.id.as_str()).copied().unwrap_or(0));
    }
}

/// One page, read and parsed once.
///
/// `store::list_pages` already reads and splits every file, and the deriver used
/// to read each one a *second* time for its links. Carrying the parse is not
/// only cheaper: it is what makes the frontmatter `edges:` array reachable at
/// all, and that array is the primary typed grammar.
struct LoadedPage {
    page: PageRef,
    node_id: String,
    doc: ConceptDoc,
    /// The whole file, frontmatter included — see the module header.
    text: String,
    /// The markdown after the frontmatter block.
    body: String,
}

/// Every page under `knowledge/`, **scaffold pages included** — [`derive`] drops
/// them and [`bundle_links`] keeps them as link *sources*.
///
/// The filter used to live here, which made the two indistinguishable. They are
/// not: `knowledge/index.md` must never be a graph node (it links to everything,
/// so it draws as a hub that crowds out the real structure), and it must still
/// count as an inbound link, because the orphan rule's own advice is "link it
/// from a hub page".
fn load_pages(kb_root: &Path) -> Result<Vec<LoadedPage>> {
    let mut out = Vec::new();
    for page in store::list_pages(kb_root, None)? {
        let text = std::fs::read_to_string(kb_root.join(&page.path))?;
        // DR-7: nothing rejects a page on read. `Page::parse` fails on exactly
        // one input — an unterminated `---` block — and the answer there is a
        // page with no typed view, not a base that refuses to derive.
        let (doc, body) = match okf::Page::parse(&text) {
            Ok(parsed) => (parsed.doc, parsed.body),
            Err(_) => (ConceptDoc::default(), text.clone()),
        };
        out.push(LoadedPage {
            node_id: path_to_node_id(&page.path),
            page,
            doc,
            text,
            body,
        });
    }
    Ok(out)
}

fn node_for(p: &LoadedPage, today: NaiveDate) -> GraphNode {
    GraphNode {
        id: p.node_id.clone(),
        // `label` stays exactly what the page list has always shown. The typed
        // display identity is `identifier`, kept separate so a renderer can
        // change what it shows without changing what an edge resolves against.
        label: p.page.title.clone(),
        kind: page_kind_of(&p.page),
        credibility_tier: None,
        retracted: false,
        path: p.page.path.clone(),
        node_type: non_empty(&p.doc.r#type),
        subtype: p.doc.subtype.clone(),
        identifier: p.doc.primary_key().map(str::to_string),
        // Only when the page states one: OKF §5.4 says an absent `status` reads
        // as `stable`, and stamping `stable` onto every legacy node would turn a
        // consumer's assumption into the producer's assertion.
        status: p.doc.status.as_ref().map(|s| s.as_str().to_string()),
        stale: okf::is_stale(&p.doc, today),
        external: false,
        // Filled by `apply_degrees` once the edges exist and have been
        // deduplicated; there is nothing to count from at this point.
        degree: None,
    }
}

/// Both spellings of the source directory, newest first.
///
/// ⚠ **OKF's is the SINGULAR `knowledge/source/`; only the pre-OKF layout used
/// the plural.** A fallback that names one of them answers for the other's
/// bases by accident. This one was plural-only, which is the layout no base
/// this build creates uses, so an OKF source page carrying no `raw_source`
/// anchor resolved to nothing at all — and `schema_okf.md` only tells the model
/// to "create the source's own page", so a page without that anchor is an
/// ordinary outcome rather than a malformed one.
pub(crate) fn source_page_candidates(raw_id: &str) -> [String; 2] {
    [
        format!("knowledge/source/{raw_id}.md"),
        format!("knowledge/sources/{raw_id}.md"),
    ]
}

/// Source nodes inherit credibility from `raw/<id>/meta.yaml`.
///
/// ⚠ **Which node stands for a raw source is decided by
/// [`source_anchor`](crate::knowledge::source_anchor), not here**, and it has to
/// be shared: `macros::lint` asks the identical question for its `stale_sources`
/// and `missing_concept_pages` rules, and the two answering it separately is how
/// this bug lasted as long as it did.
///
/// This used to look for `knowledge/sources/<id>.md` and nothing else, which is
/// the pre-OKF layout and the one layout no base created by this build uses. So
/// on every base the OKF work introduced, no node ever matched,
/// `credibility_tier` and `retracted` stayed unset, and a knowledge base citing
/// a retracted paper drew identically to one that did not — while lint's
/// provenance pass, which reads `raw/<id>/meta.yaml` directly, flagged it. Two
/// surfaces disagreeing about the same paper.
///
/// The first repair read BioOKF's `raw_source` anchor and kept the path as a
/// fallback. That covered BioOKF and left **plain OKF — the default for a new
/// base — exactly as broken as before**, because `raw_source` is a BioOKF-only
/// key (both of its writers are gated on `KbFormat::is_biookf`) and OKF's
/// directory is the SINGULAR `knowledge/source/`. `source_anchor` reads whichever
/// spelling the page actually carries; its header has the table and the reasons.
///
/// The legacy path stays as a fallback: bases created before the OKF work do use
/// it, and they are exactly the ones with no frontmatter anchor to read.
fn apply_source_credibility(
    kb_root: &Path,
    pages: &[LoadedPage],
    nodes: &mut [GraphNode],
) -> Result<()> {
    // `nodes` is built by mapping over `pages` in order, so the index is shared.
    let mut by_raw_id: HashMap<String, usize> = HashMap::new();
    for (i, page) in pages.iter().enumerate() {
        for id in crate::knowledge::source_anchor::raw_ids_stood_for(&page.page.path, &page.doc) {
            // `or_insert`, so the FIRST page in load order wins when two stand
            // for one document. Unlike lint — which reports both, because "is
            // this source cited?" is true if either is — a credibility tier can
            // only be attached to one node, and load order is sorted by path, so
            // the choice is at least stable between runs.
            by_raw_id.entry(id).or_insert(i);
        }
    }

    for src in raw::list_sources(kb_root)? {
        let target = by_raw_id.get(&src.id).copied().or_else(|| {
            source_page_candidates(&src.id)
                .iter()
                .find_map(|cand| nodes.iter().position(|n| &n.path == cand))
        });
        if let Some(i) = target {
            if let Some(n) = nodes.get_mut(i) {
                n.credibility_tier = Some(src.credibility.tier);
                n.retracted = src.credibility.retracted;
            }
        }
    }
    Ok(())
}

// ── DR-3's identity ladder ──────────────────────────────────────────────────

/// The three rungs an edge target is resolved against, in order: `identifier`,
/// then `title`, then the slugified basename.
///
/// DR-3's fourth and highest rung, `br_page_id`, is **deferred** (DR-22): it is
/// the only part of this work that would rewrite frontmatter on pages that
/// already exist, and the three rungs below it are what make a rename survivable
/// without one. A page renamed from `il6.md` to `interleukin-6.md` keeps its
/// `identifier`, so every inbound edge written against that identifier still
/// finds it — which is the whole point of the ladder.
///
/// ⚠ **The order is DR-3's and it is not free.** A base where one page's *title*
/// slugs to another page's *basename* now resolves that name to the first,
/// where the basename-only resolver chose the second. That is the deliberate
/// trade: naming a concept by its name has to beat naming it by its filename, or
/// `edges: [{object: Interleukin-6}]` — which BioOKF §7.2 defines as naming the
/// target's `identifier` — could never find a page stored under a short stem.
struct NodeIndex {
    by_identifier: HashMap<String, String>,
    by_title: HashMap<String, String>,
    by_basename: LinkIndex<String>,
}

impl NodeIndex {
    /// Built from the pages a link may resolve *to*, which is every page that
    /// becomes a node — so scaffold pages are skipped here and not only in
    /// [`derive`]. A no-op for `derive`, which has already dropped them, and
    /// load-bearing for [`bundle_links`], which passes the whole tree so that
    /// scaffold pages can still be link sources.
    fn build(pages: &[LoadedPage]) -> Self {
        let targets = || pages.iter().filter(|p| !is_scaffold_page(&p.page.path));
        let mut by_identifier = HashMap::new();
        let mut by_title = HashMap::new();
        for p in targets() {
            insert_name(&mut by_identifier, p.doc.identifier.as_deref(), &p.node_id);
            insert_name(&mut by_title, p.doc.title.as_deref(), &p.node_id);
        }
        Self {
            by_identifier,
            by_title,
            by_basename: LinkIndex::from_pages(
                targets()
                    .map(|p| (p.page.path.clone(), p.node_id.clone()))
                    .collect::<Vec<_>>(),
            ),
        }
    }

    fn resolve(&self, target: &str) -> Option<&String> {
        let name = links::identity_key(target);
        self.by_identifier
            .get(&name)
            .or_else(|| self.by_title.get(&name))
            .or_else(|| self.by_basename.resolve(target))
    }
}

/// Last writer wins on a collision, which is what [`LinkIndex`] has always done.
/// Making it an error would fail bases that exist today; making it a multi-map
/// would make "which page does this name?" unanswerable, which is the question
/// the ladder exists to answer.
fn insert_name(map: &mut HashMap<String, String>, name: Option<&str>, node_id: &str) {
    if let Some(key) = name.map(links::identity_key).filter(|k| !k.is_empty()) {
        map.insert(key, node_id.to_string());
    }
}

// ── Edge collection ─────────────────────────────────────────────────────────

/// A link target as the grammar handed it over: what it points *at*, and what
/// the page *called* it.
///
/// The two are the same string in the two grammars that name a concept
/// (`edges: [{object: Neutropenia}]`, `[[causes:: Neutropenia]]`) and different
/// in the one that names a file — an OKF §6.1 markdown link points at
/// `/knowledge/concepts/neutropenia.md` and calls it `Neutropenia`. Carrying
/// both is what lets an unresolved target be drawn under its name instead of
/// under its path; see [`external_display_name`].
///
/// A struct rather than a seventh parameter on [`EdgeCollector::push`]: the two
/// halves are only ever meaningful together, and separating them is how the
/// path came to be used as a label in the first place.
#[derive(Clone, Copy)]
struct WrittenTarget<'a> {
    /// The destination exactly as written — a path, a title, or an identifier.
    target: &'a str,
    /// The markdown link text or the wiki-link alias, when the grammar has one.
    text: Option<&'a str>,
}

impl<'a> WrittenTarget<'a> {
    /// A target that names itself: the frontmatter `edges:` object, which
    /// BioOKF §7.2 defines as the target node's `identifier`.
    fn named(target: &'a str) -> Self {
        Self { target, text: None }
    }
}

/// Where an edge points, once the ladder has had its say.
enum Target {
    /// A page in this bundle.
    Page(String),
    /// No page in this bundle carries this target. `placeholder` is the node id
    /// to draw an `external` node under, or `None` when the deriver must not
    /// invent one — a dangling legacy `[[…]]` link, see the module header.
    ///
    /// The two used to be one `External(String)` variant and a fall through to
    /// `Drop`, which made "unresolved" unaskable: the *drawing* decision had
    /// eaten the *resolution* fact. Lint's missing-concept rule needs the fact
    /// and not the decision, because on a legacy base every unresolved target is
    /// on the branch that draws nothing.
    Unresolved { placeholder: Option<String> },
    /// Nothing to record at all — a link at the bundle's own scaffold files, or
    /// a self-link. A page that mentions its own name would otherwise get a loop
    /// that says nothing.
    Drop,
}

#[derive(Default)]
struct EdgeCollector {
    edges: Vec<GraphEdge>,
    /// Keyed by node id so two pages linking to the same missing concept share
    /// one placeholder. A `BTreeMap` rather than a `HashMap` because the graph
    /// is cached on disk and compared in tests: iteration order has to be a
    /// function of the content and not of the hasher's seed.
    external: BTreeMap<String, GraphNode>,
    /// `(source node id, target exactly as written)` for every link that named
    /// no page in this bundle, in the order the pages were walked. [`derive`]
    /// ignores it; [`bundle_links`] hands it to lint's missing-concept rule,
    /// which has to see the same non-resolutions the ladder saw or it reports a
    /// different set from the graph drawn beside it.
    unresolved: Vec<(String, String)>,
}

impl EdgeCollector {
    fn collect_page(&mut self, p: &LoadedPage, index: &NodeIndex) {
        // BioOKF §6 first, because DR-25's tie-break is first-wins: a typed
        // entry has to reach `dedupe` before the prose link that restates it.
        for e in &p.doc.edges {
            self.push(
                index,
                &p.node_id,
                WrittenTarget::named(&e.object),
                non_empty(&e.predicate),
                attrs_from_frontmatter(e),
                true,
            );
        }
        self.collect_body_links(p, index);
    }

    fn collect_body_links(&mut self, p: &LoadedPage, index: &NodeIndex) {
        for link in links::wiki_links(&p.text) {
            // Only the sugar form records a dangling target; the legacy form is
            // what a pre-Stage-2 base is written in. See the module header.
            let record_dangling = link.form == LinkForm::BioOkfEdgeSugar;
            self.push(
                index,
                &p.node_id,
                WrittenTarget {
                    target: &link.target,
                    text: link.label.as_deref(),
                },
                link.predicate.clone(),
                attrs_from_sugar(&link),
                record_dangling,
            );
        }
        for link in okf::extract_links(&p.body) {
            // `is_bundle_link` is what keeps every page that cites a paper from
            // gaining an edge to `https://pubmed.ncbi.nlm.nih.gov`.
            if link.form == LinkForm::OkfMarkdown && link.is_bundle_link() {
                self.push(
                    index,
                    &p.node_id,
                    WrittenTarget {
                        target: &link.target,
                        text: link.label.as_deref(),
                    },
                    None,
                    EdgeAttrs::default(),
                    true,
                );
            }
        }
    }

    fn push(
        &mut self,
        index: &NodeIndex,
        from: &str,
        written: WrittenTarget<'_>,
        predicate: Option<String>,
        attrs: EdgeAttrs,
        record_dangling: bool,
    ) {
        let to = match classify_target(index, from, written.target, record_dangling) {
            Target::Page(id) => id,
            Target::Unresolved { placeholder } => {
                self.unresolved
                    .push((from.to_string(), written.target.to_string()));
                let Some(id) = placeholder else { return };
                self.ensure_external(&id, written);
                id
            }
            Target::Drop => return,
        };
        self.edges
            .push(make_edge(from, to, predicate, attrs, index));
    }

    /// The placeholder for an unresolved target.
    ///
    /// **Keyed on the id, named by the first page to reach it.** The id comes
    /// from the target (`links::link_key`), so two pages naming the same missing
    /// concept — by identifier, by path, in either grammar — still share one
    /// node; only the *display* name is first-writer-wins, and it is a name for
    /// the same thing either way.
    fn ensure_external(&mut self, id: &str, written: WrittenTarget<'_>) {
        let name = external_display_name(written);
        self.external
            .entry(id.to_string())
            .or_insert_with(|| GraphNode {
                id: id.to_string(),
                label: name.clone(),
                // The least-loaded value in the enum. An external node's own
                // flag is what a renderer keys on; `kind` is a *page* taxonomy
                // and this node has no page, so any answer here is a guess and
                // the honest guess is the neutral one.
                kind: PageKind::Note,
                credibility_tier: None,
                retracted: false,
                // No file to open, and the empty string says so at every reader
                // rather than at one that remembered to check `external`.
                path: String::new(),
                node_type: None,
                subtype: None,
                identifier: Some(name),
                status: None,
                stale: false,
                external: true,
                // As in `node_for`: `apply_degrees` fills it once the edges are
                // deduplicated. An external node is by construction incident to
                // at least one, so this never stays `Some(0)`.
                degree: None,
            });
    }
}

/// What to call a node that has no page.
///
/// The gate found this drawing a **file path** in the graph: a dangling
/// `[Nonexistent Thing](/knowledge/concept/nonexistent-thing.md)` produced a node
/// labelled `/knowledge/concept/nonexistent-thing.md`, because the deriver used
/// the link's *destination* where the page had supplied a perfectly good name
/// next to it. The destination is right for identity — it is what
/// `links::link_key` keys the node on — and wrong for display.
///
/// So: the link text where the grammar supplies one (markdown label, wiki-link
/// alias), and the target itself otherwise, since the two grammars with no text
/// name the concept directly (BioOKF §7.2's `edges: [{object}]` is the target's
/// `identifier`).
///
/// **The basename is humanised only for a path-shaped target**, and that guard
/// is the whole subtlety. `-` is punctuation in `nonexistent-thing.md` and part
/// of the name in `COVID-19`; a page names the second form far more often than
/// the first, so an unconditional separator swap would rewrite more names than
/// it repaired. Case is never touched either — title-casing would turn the gene
/// symbol `il6` into `Il6`, and a symbol's case is information, not formatting.
fn external_display_name(written: WrittenTarget<'_>) -> String {
    if let Some(text) = written.text.map(str::trim).filter(|t| !t.is_empty()) {
        return text.to_string();
    }
    let target = written.target.trim();
    if !links::written_as_path(target) {
        return target.to_string();
    }
    let basename = target
        .rsplit('/')
        .next()
        .unwrap_or(target)
        .trim_end_matches(".md")
        .trim();
    let humanised = basename
        .chars()
        .map(|c| if c == '-' || c == '_' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string();
    // A target that is all separators (`/`, `--.md`) leaves nothing readable;
    // showing what was written beats showing an empty label.
    if humanised.is_empty() {
        target.to_string()
    } else {
        humanised
    }
}

fn classify_target(index: &NodeIndex, from: &str, written: &str, record_dangling: bool) -> Target {
    match index.resolve(written) {
        Some(to) if to == from => Target::Drop,
        Some(to) => Target::Page(to.clone()),
        // The scaffold check sits on the *dangling* branch alone, on purpose. A
        // real page at `knowledge/topics/index.md` is a node, and a link naming
        // it resolves above; checking the name first would drop that edge. It is
        // also not "unresolved": the bundle's own furniture is not a concept the
        // base is missing a page for.
        None if is_scaffold_target(written) => Target::Drop,
        None => {
            let key = links::link_key(written);
            // `external::` cannot collide with a page-derived id: those are
            // path segments joined by a single `:`, and a walked directory
            // never yields an empty segment.
            let placeholder =
                (record_dangling && !key.is_empty()).then(|| format!("external::{key}"));
            Target::Unresolved { placeholder }
        }
    }
}

fn make_edge(
    from: &str,
    to: String,
    predicate: Option<String>,
    attrs: EdgeAttrs,
    index: &NodeIndex,
) -> GraphEdge {
    // Two spellings of the same fact (SPEC §6.F's canonical `not_<X>` and the
    // legacy `negated: true` attribute), resolved here so no reader downstream
    // has to know there were two.
    let negated = attrs.negated
        || predicate
            .as_deref()
            .is_some_and(|p| p.starts_with(NEGATION_PREFIX));
    GraphEdge {
        from: from.to_string(),
        to,
        relation: predicate.clone(),
        predicate,
        negated,
        knowledge_level: attrs.knowledge_level,
        agent_type: attrs.agent_type,
        // A node id when it resolves, and the identifier verbatim when it does
        // not — an unresolved source is something lint must be able to name, and
        // a deriver that blanks it leaves lint nothing to name it with.
        primary_source: attrs
            .primary_source
            .map(|s| index.resolve(&s).cloned().unwrap_or(s)),
        publications: attrs.publications,
        quantitative: attrs.quantitative,
        qualifiers: attrs.qualifiers,
        // Always false here: the deriver emits only edges a page states, and
        // DR-24 puts the `reported_in` emission in BioOKF-mode ingest. See
        // [`GraphEdge::synthesized`].
        synthesized: false,
    }
}

/// DR-25: one relationship, one edge.
///
/// Two passes, because the two rules are different. The first collapses exact
/// repeats — the same `(from, to, predicate)` written twice — keeping the first,
/// which is why `collect_page` reads the frontmatter array before the prose. The
/// second is the absorption: an untyped link is dropped when a typed edge
/// already joins the same pair, so a page that says `treats` in `edges:` and
/// links to the same target in prose renders one typed edge and not a typed one
/// with a grey twin beside it.
fn dedupe(edges: Vec<GraphEdge>) -> Vec<GraphEdge> {
    let mut seen = HashSet::new();
    let mut kept: Vec<GraphEdge> = edges
        .into_iter()
        .filter(|e| seen.insert((e.from.clone(), e.to.clone(), e.predicate.clone())))
        .collect();
    let typed: HashSet<(String, String)> = kept
        .iter()
        .filter(|e| e.predicate.is_some())
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    kept.retain(|e| e.predicate.is_some() || !typed.contains(&(e.from.clone(), e.to.clone())));
    kept
}

// ── Edge attributes ─────────────────────────────────────────────────────────

/// BioOKF §7.3's quantitative slots: the keys that go to
/// [`GraphEdge::quantitative`] rather than to [`GraphEdge::qualifiers`].
///
/// **The key decides the map; the value decides the type** ([`QuantitativeValue`]).
/// Splitting on the value instead is the bug worth naming, because it is the
/// obvious implementation: `p_value: 3.0e-6` would land in `quantitative` and
/// `p_value: <0.001` — the same slot, written by the same model, on the next
/// page — would land in `qualifiers`. One key must not be able to appear in two
/// maps depending on how precisely someone measured it.
///
/// An unrecognised key falls through to `qualifiers`, which is also the residue
/// map. That direction is the conservative one: §7.2 context wrongly filed as a
/// statistic is the category error DR-27 names, while a statistic sitting in the
/// residue map still renders — §4.8 draws every key in both maps the same way,
/// and its two key-sensitive behaviours (the `ci_lower`/`ci_upper` merge and the
/// headline stat pick) name slots that are in this list.
///
/// `direction` and `aspect` are deliberately **not** here. They are BioOKF §7.2
/// typed edge attributes — Biolink's `object_direction_qualifier` and
/// `object_aspect_qualifier` — and [`attrs_from_frontmatter`] has always filed
/// them as qualifiers. UI spec §4.8's stat pick lists `direction` under
/// `quantitative`; this build does not move it there, so that pick falls through
/// to the next slot it names.
const QUANTITATIVE_KEYS: &[&str] = &[
    "adjusted_p_value",
    "auc",
    "ci_lower",
    "ci_upper",
    "clinical_phase",
    "effect_metric",
    "effect_size",
    "frequency",
    "p_value",
    "response_direction",
    "sample_size",
    "sensitivity",
    "specificity",
    "standard_error",
    "unit",
];

/// The BioOKF §7.2/§7.3 attributes this build models, plus everything else as
/// text. One type for both attributed grammars, so the frontmatter array and the
/// inline sugar cannot come to disagree about what `p_value` means.
#[derive(Default)]
struct EdgeAttrs {
    knowledge_level: Option<String>,
    agent_type: Option<String>,
    primary_source: Option<String>,
    publications: Vec<String>,
    negated: bool,
    quantitative: BTreeMap<String, QuantitativeValue>,
    qualifiers: BTreeMap<String, String>,
}

impl EdgeAttrs {
    fn absorb(&mut self, key: &str, value: &str) {
        let v = value.trim();
        if v.is_empty() {
            return;
        }
        if self.absorb_named(key, v) {
            return;
        }
        if QUANTITATIVE_KEYS.contains(&key) {
            self.quantitative
                .insert(key.to_string(), QuantitativeValue::parse(v));
            return;
        }
        self.qualifiers.insert(key.to_string(), v.to_string());
    }

    fn absorb_named(&mut self, key: &str, v: &str) -> bool {
        match key {
            "knowledge_level" => self.knowledge_level = Some(v.to_string()),
            "agent_type" => self.agent_type = Some(v.to_string()),
            "primary_source" => self.primary_source = Some(v.to_string()),
            "publications" => self.publications.extend(
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            ),
            "negated" => self.negated |= matches!(v, "true" | "yes" | "1"),
            _ => return false,
        }
        true
    }
}

fn attrs_from_frontmatter(e: &OkfEdge) -> EdgeAttrs {
    let mut attrs = EdgeAttrs {
        knowledge_level: e.knowledge_level.clone(),
        agent_type: e.agent_type.clone(),
        primary_source: e.primary_source.clone(),
        publications: e.publications.clone(),
        negated: e.negated.unwrap_or(false),
        ..EdgeAttrs::default()
    };
    for (key, value) in [("direction", &e.direction), ("aspect", &e.aspect)] {
        if let Some(v) = value {
            attrs.qualifiers.insert(key.to_string(), v.clone());
        }
    }
    // Everything BioOKF names that this build does not model, and everything it
    // names after this build shipped. `ConceptDoc` preserves them; dropping them
    // here would make preservation pointless for anything reading the graph.
    for (key, value) in &e.extra {
        if let (Some(k), Some(v)) = (yaml_text(key), yaml_text(value)) {
            attrs.absorb(&k, &v);
        }
    }
    attrs
}

fn attrs_from_sugar(link: &LinkRef) -> EdgeAttrs {
    let mut attrs = EdgeAttrs::default();
    for (key, value) in &link.attributes {
        attrs.absorb(key, value);
    }
    attrs
}

/// A YAML scalar as the text a producer wrote, or `None` for a shape that has no
/// single-line reading. A sequence joins with `, ` because that is how BioOKF's
/// own list-valued attributes read in prose.
fn yaml_text(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Sequence(items) => {
            let parts: Vec<String> = items.iter().filter_map(yaml_text).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        _ => None,
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.trim().is_empty()).then(|| s.trim().to_string())
}
/// The shape of the graph as this build writes and reads it. Bump this whenever
/// [`Graph`], [`GraphNode`] or [`GraphEdge`] changes shape in a way that makes
/// an older cache wrong rather than merely incomplete — a new node field the
/// deriver now fills, a changed id scheme, a dropped node class.
///
/// Version 1 is the first *stated* version. Caches written before the envelope
/// existed are bare `Graph` JSON with no `version` key at all; they fail to
/// deserialize into [`CacheEnvelope`] and are therefore treated as absent, which
/// is exactly the treatment they need.
///
/// **Version 2 is Stage 2's typed graph**, and it is the case the envelope was
/// landed a stage early for. Every new node and edge field is `#[serde(default)]`
/// — it has to be, or a stale cache would fail to *read* — so a v1 cache would
/// otherwise deserialize cleanly and serve typeless nodes for ever, on every
/// base that already exists, with the change appearing to work and producing
/// nothing. The version is what makes that impossible: a v1 cache is absent, so
/// the base re-derives once and is typed from then on.
///
/// **Version 3 is DR-27's payload**: the six flat statistical fields became the
/// open `quantitative` map, and `GraphNode::degree` arrived. Both are the same
/// hazard one turn further on, and the second is the quieter of the two — a v2
/// cache carries the six old keys under names nothing reads any more *and* no
/// degree at all, so a renderer sizing hubs by `degree` would draw every node in
/// every pre-existing base at the same size and look like a layout bug.
const CACHE_VERSION: u32 = 3;

/// The on-disk envelope, so `graph-cache.json` says what it is.
///
/// A bare `Graph` on disk cannot answer "was this written by a deriver that
/// knows about the fields I am about to read?", and DR-13 records both ways
/// that ends badly. The loud way: `read_cache` was
/// `Ok(Some(serde_json::from_str(&s)?))`, so a shape change made every existing
/// base's cache a deserialize **error**, which propagated out of
/// `GET /knowledge/bases/{id}/graph` as a 404 that nothing on that path ever
/// repaired — a permanent error in the Knowledge view, healed only by deleting
/// a file the user cannot see. The quiet way is worse: give the new fields
/// `#[serde(default)]` and every stale cache deserializes cleanly, so every
/// pre-existing base serves empty/typeless nodes forever and the change appears
/// to work while producing nothing.
///
/// Generic over the payload so the write side can serialize a `&Graph` without
/// cloning it while the read side deserializes an owned one.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEnvelope<G> {
    version: u32,
    graph: G,
}

/// Where a base's cache lives, spelled once.
///
/// It was spelled four times — twice here and twice in this module's tests —
/// which was survivable while this file was the only one that touched the file.
/// It stopped being so when `git::abort_txn` gained a reason to *delete* a cache
/// it could not rebuild: a fifth hand-written copy of the path in another module
/// is one that a rename would leave pointing at nothing, silently, on the error
/// path of an error path.
pub(crate) fn cache_path(kb_root: &Path) -> std::path::PathBuf {
    kb_root
        .join(".biorouter-knowledge")
        .join("graph-cache.json")
}

pub fn write_cache(kb_root: &Path, graph: &Graph) -> Result<()> {
    let path = cache_path(kb_root);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let envelope = CacheEnvelope {
        version: CACHE_VERSION,
        graph,
    };
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&envelope)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// The cached graph, or `None` meaning "no usable cache — re-derive".
///
/// **This function never reports a bad cache as an error**, and that is the
/// whole point of DR-13. A cache is an optimisation over `derive()`, which can
/// always be run again; every way of failing to read one — missing file, unreadable
/// file, malformed JSON, a version this build does not know — has the same
/// correct answer, and it is `Ok(None)`. Returning `Err` here instead put a
/// permanent 404 in front of the Knowledge view (see [`CacheEnvelope`]).
///
/// The `Result` in the signature is kept for the callers, not for the failures:
/// it is where a future durable-read error would go, and dropping it would churn
/// every call site for no gain.
pub fn read_cache(kb_root: &Path) -> Result<Option<Graph>> {
    let path = cache_path(kb_root);
    if !path.exists() {
        return Ok(None);
    }
    let s = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "knowledge: graph cache at {} unreadable, re-deriving: {e}",
                path.display()
            );
            return Ok(None);
        }
    };
    match serde_json::from_str::<CacheEnvelope<Graph>>(&s) {
        Ok(envelope) if envelope.version == CACHE_VERSION => Ok(Some(envelope.graph)),
        Ok(envelope) => {
            tracing::info!(
                "knowledge: graph cache at {} is version {}, this build writes {CACHE_VERSION}; re-deriving",
                path.display(),
                envelope.version
            );
            Ok(None)
        }
        Err(e) => {
            // Includes every cache written before the envelope existed: those
            // files are a bare `Graph` object with no `version` key.
            tracing::info!(
                "knowledge: graph cache at {} is not in a shape this build reads, re-deriving: {e}",
                path.display()
            );
            Ok(None)
        }
    }
}

/// Scaffold pages that exist in every KB and should never appear as graph
/// nodes: the auto-maintained index, the change log, and the maintenance
/// schema. Matched on the logical path so a real page that merely contains the
/// word "index" is unaffected.
///
/// `schema.md` joins them at Stage 3 and the reason is DR-23. OKF §3.1 reserves
/// exactly `index.md` and `log.md` and then says "All other `.md` files are
/// concept documents", so BioRouter's `schema.md` now carries `type: Schema` —
/// which makes it, to every rule in `okf::conformance`, an ordinary typed
/// concept document. It is a *scaffold* page all the same: it describes how to
/// write the base, it is not a thing the base knows about, and as a node it
/// would be a hub linked from nothing.
///
/// Today this is belt-and-braces — `store::list_pages` walks `knowledge/` only,
/// and `schema.md` sits at the bundle root — and that is exactly why it is
/// written down: the day the walker widens to the whole bundle (which OKF's own
/// "every non-reserved `.md`" framing invites), a typed `schema.md` would
/// otherwise arrive in the graph as a `Schema` node with no warning.
pub(crate) fn is_scaffold_page(logical: &str) -> bool {
    matches!(
        logical,
        "knowledge/index.md"
            | "knowledge/log.md"
            | "knowledge/schema.md"
            | "index.md"
            | "log.md"
            | "schema.md"
    )
}

/// A link target naming a scaffold page, checked *only* on the dangling branch
/// (see [`classify_target`]).
///
/// Reduced through the shared basename key so `[index]`, `/knowledge/index.md`
/// and `index.md` are one answer. A real page whose own basename is `index` —
/// `knowledge/topics/index.md`, which is not scaffold — is a node and resolves
/// before this is ever consulted.
fn is_scaffold_target(target: &str) -> bool {
    matches!(links::link_key(target).as_str(), "index" | "log" | "schema")
}

fn path_to_node_id(logical: &str) -> String {
    logical
        .strip_prefix("knowledge/")
        .unwrap_or(logical)
        .trim_end_matches(".md")
        .replace('/', ":")
}

fn page_kind_of(p: &PageRef) -> PageKind {
    match (p.kind.as_str(), p.path.as_str()) {
        ("source", _) => PageKind::Source,
        ("entity", _) => PageKind::Entity,
        ("concept", _) => PageKind::Concept,
        ("hub", _) => PageKind::Hub,
        ("flag", _) => PageKind::Flag,
        (_, path) if path.starts_with("knowledge/sources/") => PageKind::Source,
        (_, path) if path.starts_with("knowledge/entities/") => PageKind::Entity,
        (_, path) if path.starts_with("knowledge/concepts/") => PageKind::Concept,
        (_, path) if path.starts_with("knowledge/notes/") => PageKind::Note,
        _ => PageKind::Hub,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;
    use crate::knowledge::store::write_page;

    fn build_sample() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntitle: HRV\nkind: entity\n---\nLinks to [[zone-2 base]].",
            "add hrv",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/concepts/zone-2 base.md",
            "---\ntitle: Zone-2 base\nkind: concept\n---\nLinks to [[hrv]].",
            "add z2",
            None,
        )
        .unwrap();
        (dir, kb)
    }

    #[test]
    fn derives_nodes_and_edges() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 2, "bidirectional links → two edges");
        let labels: Vec<_> = g.nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"HRV"));
        assert!(labels.contains(&"Zone-2 base"));
    }

    #[test]
    fn excludes_index_and_log_scaffold_pages() {
        let (_d, kb) = build_sample();
        // Simulate the auto-maintained scaffold pages that the sub-agent keeps
        // up to date; they link to everything and must not become nodes.
        write_page(
            &kb,
            "knowledge/index.md",
            "---\ntitle: Index\nkind: hub\n---\nLinks [[hrv]] and [[zone-2 base]].",
            "add index",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/log.md",
            "---\ntitle: Log\nkind: hub\n---\nchange log",
            "add log",
            None,
        )
        .unwrap();
        let g = derive(&kb).unwrap();
        assert!(
            g.nodes.iter().all(|n| n.id != "index" && n.id != "log"),
            "scaffold pages must be excluded, got {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        assert_eq!(g.nodes.len(), 2, "only the two real pages remain");
        // Edges pointing at the excluded index must be dropped.
        assert!(g.edges.iter().all(|e| e.to != "index" && e.from != "index"));
    }

    /// DR-23: `schema.md` now carries `type: Schema`, which makes it — to every
    /// rule in `okf::conformance` and to the deriver's own typed reader — an
    /// ordinary concept document. It must still be skipped as scaffold.
    ///
    /// Two halves, because only one of them can fail today. The bundle-root
    /// `schema.md` a real base is created with is out of `list_pages`' reach
    /// (it walks `knowledge/` only), so the first half asserts the *state* is
    /// right rather than that a filter fired. The second half puts a typed
    /// `schema.md` where the walker does look, and that one would arrive in the
    /// graph as a `Schema` node without the filter.
    #[test]
    fn a_typed_schema_md_is_scaffold_and_never_becomes_a_node() {
        let (_d, kb) = build_sample();
        let schema = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert!(
            schema.starts_with("---\ntype: Schema\n"),
            "the premise of this test is gone: {}",
            schema.lines().take(3).collect::<Vec<_>>().join(" / ")
        );

        write_page(
            &kb,
            "knowledge/schema.md",
            "---\ntype: Schema\ntitle: Schema\n---\nHow to maintain this base.",
            "add a schema page inside knowledge/",
            None,
        )
        .unwrap();
        let g = derive(&kb).unwrap();
        assert!(
            g.nodes.iter().all(|n| n.id != "schema"),
            "schema.md became a node: {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        assert!(
            g.nodes.iter().all(|n| !n.path.ends_with("schema.md")),
            "a schema page reached the graph under another id"
        );
        assert_eq!(g.nodes.len(), 2, "only the two real pages remain");
    }

    /// …and a link *naming* the schema resolves to nothing rather than
    /// inventing an `external` node for it, exactly as a link naming the index
    /// or the log does. The three scaffold files are the base's own furniture,
    /// not concepts it is missing a page for.
    #[test]
    fn a_link_to_a_scaffold_file_invents_no_external_node() {
        let (_d, kb) = build_sample();
        write_page(
            &kb,
            "knowledge/a.md",
            "---\ntitle: A\n---\nSee [the schema](/schema.md), [the index](/index.md) and [the log](/log.md).\n",
            "link at the scaffold",
            None,
        )
        .unwrap();
        let g = derive(&kb).unwrap();
        assert!(
            g.nodes.iter().all(|n| !n.external),
            "a scaffold link became an external node: {:?}",
            g.nodes
                .iter()
                .filter(|n| n.external)
                .map(|n| &n.id)
                .collect::<Vec<_>>()
        );
    }

    /// Write whatever bytes over the cache file, bypassing [`write_cache`] —
    /// the only way to express "a file an older build left behind".
    fn overwrite_cache_file(kb: &std::path::Path, bytes: &str) {
        let path = cache_path(kb);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// DR-13, and the case every knowledge base on disk is in right now: its
    /// cache is a bare `Graph` object with no `version` key, because it was
    /// written before the envelope existed. It must read as *absent* — silently
    /// re-derived — and not as the `Err` that used to leave `GET
    /// /knowledge/bases/{id}/graph` answering 404 forever.
    ///
    /// This test replaces `get_graph_self_heals_stale_cache_with_scaffold_nodes`.
    /// The v0 cache it writes contains the scaffold `index` node that test used
    /// as its fingerprint, so the old assertion is made here too — the version
    /// check subsumes the hardcoded predicate rather than merely coexisting
    /// with it.
    #[test]
    fn a_v0_shaped_cache_is_treated_as_absent_and_silently_re_derived() {
        use crate::knowledge::types::{Graph, GraphNode, PageKind};
        let (_d, kb) = build_sample();
        // Exactly what the pre-envelope `write_cache` produced: the graph
        // itself, serialized at the top level.
        let v0 = Graph {
            nodes: vec![GraphNode {
                id: "index".into(),
                label: "Index".into(),
                kind: PageKind::Hub,
                credibility_tier: None,
                retracted: false,
                path: "knowledge/index.md".into(),
                node_type: None,
                subtype: None,
                identifier: None,
                status: None,
                stale: false,
                external: false,
                degree: None,
            }],
            edges: vec![],
        };
        overwrite_cache_file(&kb, &serde_json::to_string_pretty(&v0).unwrap());

        assert!(
            read_cache(&kb)
                .expect("a stale cache is never an error")
                .is_none(),
            "a cache with no version key must read as absent"
        );

        let svc = KnowledgeService::new(_d.path().to_path_buf());
        let g = svc.get_graph("k").unwrap();
        assert_eq!(g.nodes.len(), 2, "the two real pages were re-derived");
        assert!(
            g.nodes.iter().all(|n| n.id != "index"),
            "the scaffold node from the v0 cache survived, got {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        // And the cache on disk is healed, so the next read is served from it.
        let healed = read_cache(&kb)
            .unwrap()
            .expect("re-derive rewrote the cache");
        assert_eq!(healed, g);
    }

    #[test]
    fn a_cache_from_a_future_version_is_treated_as_absent() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        overwrite_cache_file(
            &kb,
            &serde_json::to_string(&serde_json::json!({
                "version": CACHE_VERSION + 1,
                "graph": g,
            }))
            .unwrap(),
        );
        assert!(
            read_cache(&kb)
                .expect("a newer cache is never an error")
                .is_none(),
            "a version this build does not write must read as absent"
        );
    }

    #[test]
    fn a_corrupt_cache_is_absent_rather_than_an_error() {
        // Half-written JSON is what a machine losing power mid-`write_cache`
        // leaves behind, tmp+rename notwithstanding — and a truncated file that
        // 404s the graph route forever is the failure DR-13 names.
        let (_d, kb) = build_sample();
        overwrite_cache_file(&kb, "{\"version\": 1, \"graph\": {\"nodes\": [");
        assert!(read_cache(&kb).expect("corrupt is not an error").is_none());
    }

    #[test]
    fn cache_write_then_read() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        write_cache(&kb, &g).unwrap();
        let back = read_cache(&kb).unwrap().unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn the_cache_file_states_its_own_version() {
        // Self-describing on disk, not merely in the type: a reader holding the
        // file and not this source has to be able to tell what it is.
        let (_d, kb) = build_sample();
        write_cache(&kb, &derive(&kb).unwrap()).unwrap();
        let raw = std::fs::read_to_string(cache_path(&kb)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["version"], serde_json::json!(CACHE_VERSION));
        assert!(v["graph"]["nodes"].is_array(), "got: {raw}");
    }

    #[test]
    fn derives_edges_from_piped_wiki_link_syntax() {
        // The sub-agent commonly emits Obsidian-style `[[target|alias]]`
        // links pointing at full logical paths. Both the path prefix and the
        // alias must be tolerated by the deriver.
        //
        // ⚠ **One target page per link shape, and that is load-bearing.** This
        // test used to point both shapes at *one* page and count the edges
        // arriving there, expecting two. DR-25 then made the deriver reduce to a
        // set keyed on `(from, to, predicate)` — "in OKF mode, where there are
        // no predicates, links to the same target collapse to one edge" — so the
        // count is now 1 whether both shapes resolve or only one does, and the
        // old assertion could no longer fail. `links.rs`'s equivalence corpus
        // learned exactly this lesson first and records it at length: several
        // shapes aimed at one page cannot tell the shapes apart. Each shape gets
        // its own page, so breaking that shape orphans that page and no other
        // link covers for it.
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");

        write_page(
            &kb,
            "knowledge/entities/wanjun-gu.md",
            "---\ntitle: Wanjun Gu\nkind: entity\n---\nBody.",
            "add entity",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/entities/sergio-baranzini.md",
            "---\ntitle: Sergio Baranzini\nkind: entity\n---\nBody.",
            "add second entity",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/sources/wanjun-gu---google-scholar-d7f205.md",
            "---\ntitle: Wanjun Gu, Google Scholar (URL reference)\nkind: source\n---\n\
             References [[knowledge/entities/wanjun-gu|Wanjun Gu]] and\n\
             also [[knowledge/entities/sergio-baranzini.md|Sergio Baranzini]] (with .md).\n",
            "add source",
            None,
        )
        .unwrap();

        let g = derive(&kb).unwrap();
        assert_eq!(g.nodes.len(), 3);
        // Both piped wiki-link forms should resolve to their entity page.
        for target in ["entities:wanjun-gu", "entities:sergio-baranzini"] {
            let drawn = g.edges.iter().filter(|e| e.to == target).count();
            assert_eq!(
                drawn, 1,
                "expected the piped wiki-link to {target} to produce exactly one \
                 edge, got edges: {:?}",
                g.edges
            );
        }
    }

    // ── `bundle_links`: the link view lint reads ────────────────────────────

    /// A scaffold page is not a node and **is** a link source.
    ///
    /// The two are separate questions and the loader used to answer only the
    /// first, by dropping scaffold pages before anything could read them. The
    /// orphan rule's own advice is "link it from a hub page"; taking that advice
    /// and being told the page is still an orphan is the failure this guards.
    #[test]
    fn the_index_page_is_a_link_source_but_never_a_node() {
        let (_d, kb) = build_sample();
        write_page(
            &kb,
            "knowledge/notes/mentioned-only-by-the-index.md",
            "---\ntitle: Mentioned only by the index\nkind: note\n---\nBody.",
            "add",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/index.md",
            "---\ntitle: Index\nkind: hub\n---\nSee [[mentioned-only-by-the-index]].",
            "add index",
            None,
        )
        .unwrap();

        let links = bundle_links(&kb).unwrap();
        let inbound = links
            .inbound
            .get("knowledge/notes/mentioned-only-by-the-index.md")
            .expect("every non-scaffold page has an entry");
        assert_eq!(
            inbound.iter().collect::<Vec<_>>(),
            vec!["knowledge/index.md"],
            "the index links to it; inbound={inbound:?}"
        );
        assert!(
            !links.inbound.contains_key("knowledge/index.md"),
            "a scaffold page is not a page the orphan rule can fire on"
        );
        assert!(
            derive(&kb).unwrap().nodes.iter().all(|n| n.id != "index"),
            "and it is still not a graph node"
        );
    }

    /// A page nothing names has an entry with an **empty** set, not a missing
    /// key — "nothing links here" and "no such page" are different answers and
    /// the orphan rule is the difference between them.
    #[test]
    fn a_page_nothing_links_to_is_an_empty_set_and_not_a_missing_key() {
        let (_d, kb) = build_sample();
        write_page(
            &kb,
            "knowledge/notes/lonely.md",
            "---\ntitle: Lonely\nkind: note\n---\nBody.",
            "add",
            None,
        )
        .unwrap();
        let links = bundle_links(&kb).unwrap();
        assert_eq!(
            links.inbound.get("knowledge/notes/lonely.md"),
            Some(&HashSet::new())
        );
        assert_eq!(links.inbound.get("knowledge/notes/absent.md"), None);
    }

    /// A dangling legacy bracket link draws **no** external node — that is what
    /// keeps a legacy base's node set unchanged (see the module header) — and is
    /// still recorded as unresolved, because lint's missing-concept rule is
    /// asking about the resolution and not about the drawing. Collapsing the two
    /// is what left that rule empty.
    #[test]
    fn a_dangling_legacy_link_draws_no_node_and_is_still_unresolved() {
        let (_d, kb) = build_sample();
        write_page(
            &kb,
            "knowledge/sources/paper.md",
            "---\ntitle: Paper\nkind: source\n---\nMentions [[Nonexistent Page]].",
            "add",
            None,
        )
        .unwrap();
        assert!(
            derive(&kb).unwrap().nodes.iter().all(|n| !n.external),
            "a dangling legacy link must not invent a node"
        );
        assert!(
            bundle_links(&kb).unwrap().unresolved.contains(&(
                "knowledge/sources/paper.md".into(),
                "Nonexistent Page".into()
            )),
            "…and must still be reported as unresolved"
        );
    }

    /// A typed `edges:` object that names no page is unresolved too, which is
    /// the half a bracket-only reader could never see.
    #[test]
    fn an_unresolved_frontmatter_edge_object_is_reported() {
        let (_d, kb) = kb_with(&[(
            "knowledge/sources/paper.md",
            "---\ntype: Publication\nidentifier: Paper\nedges:\n  \
             - predicate: mentions\n    object: Long COVID\n    \
             knowledge_level: knowledge_assertion\n    agent_type: manual_agent\n    \
             primary_source: Paper\n---\n\nBody.\n",
        )]);
        let unresolved = bundle_links(&kb).unwrap().unresolved;
        assert!(
            unresolved.contains(&("knowledge/sources/paper.md".into(), "Long COVID".into())),
            "got {unresolved:?}"
        );
    }

    /// A link at the bundle's own furniture is neither an edge nor a missing
    /// concept: the base is not short of an `index.md`.
    #[test]
    fn a_link_at_the_scaffold_is_not_reported_as_unresolved() {
        let (_d, kb) = build_sample();
        write_page(
            &kb,
            "knowledge/sources/paper.md",
            "---\ntitle: Paper\nkind: source\n---\nSee [[index]], [[log]] and [[schema]].",
            "add",
            None,
        )
        .unwrap();
        assert!(
            bundle_links(&kb).unwrap().unresolved.is_empty(),
            "{:?}",
            bundle_links(&kb).unwrap().unresolved
        );
    }

    /// ⚠ **The typed layouts, which is every base this build creates.**
    ///
    /// `source_retracted_flag_propagates_to_graph_node` below hand-writes
    /// `knowledge/sources/<id>.md` — the PRE-OKF layout, and the one layout no
    /// base created by this build uses. OKF scaffolds the singular
    /// `knowledge/source/`; BioOKF writes a typed directory per source kind.
    /// So the deriver's hardcoded `knowledge/sources/` match found nothing on
    /// any base the OKF work introduced: `credibility_tier` and `retracted`
    /// stayed unset, a base citing a retracted paper drew exactly like one that
    /// did not, and lint flagged the same paper from `raw/<id>/meta.yaml`. The
    /// suite stayed green because the only fixture used the legacy path.
    ///
    /// The match is now the page's own `raw_source` anchor, so this asserts the
    /// typed spelling and the legacy test below asserts the fallback.
    /// ⚠ The SINGULAR `knowledge/source/`, with no `raw_source` anchor — which
    /// is an ordinary OKF page, not a malformed one.
    ///
    /// `schema_okf.md` only tells the model to "create the source's own page";
    /// the `raw_source` anchor is a BioOKF §7.1 key. So a plain-OKF base
    /// routinely has source pages the anchor cannot match, and the fallback is
    /// the only thing that can. It named the PLURAL `knowledge/sources/` alone —
    /// the pre-OKF layout, the one directory no base this build creates uses —
    /// so on the default format the credibility ring and the retracted badge
    /// resolved to nothing at all.
    #[test]
    fn an_okf_source_page_with_no_anchor_still_inherits_its_retraction() {
        use crate::knowledge::raw::write_raw;
        use crate::knowledge::types::{Credibility, CredibilityTier, SourceMeta};

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");

        write_raw(
            &kb,
            None,
            None,
            "# r\n",
            SourceMeta {
                id: "retracted-paper".into(),
                title: "Title".into(),
                url: Some("https://example.org/x".into()),
                ingested_at: chrono::Utc::now(),
                sha256: "abc".into(),
                mime: "text/html".into(),
                original_filename: Some("x.html".into()),
                credibility: Credibility {
                    tier: CredibilityTier::PeerReviewed,
                    confidence: 0.9,
                    publisher: None,
                    venue: None,
                    doi: None,
                    retracted: true,
                    reasoning: "test".into(),
                    classifier_version: 1,
                },
            },
        )
        .unwrap();

        // Singular directory, and deliberately NO `raw_source:` key.
        const OKF_SOURCE_PAGE: &str = "---\ntype: Source\nidentifier: Retracted Paper\n---\nbody";
        write_page(
            &kb,
            "knowledge/source/retracted-paper.md",
            OKF_SOURCE_PAGE,
            "add r",
            None,
        )
        .unwrap();

        let g = derive(&kb).unwrap();
        let node = g
            .nodes
            .iter()
            .find(|n| n.path == "knowledge/source/retracted-paper.md")
            .expect("the OKF source page must be a node");
        assert!(
            node.retracted,
            "an OKF source page at the singular path must inherit its retraction; the \
             fallback named only the pre-OKF plural directory, so on the default format \
             nothing resolved"
        );
    }

    #[test]
    fn a_typed_source_page_still_inherits_its_retraction() {
        // ⚠ A const, not an inline literal with `\` continuations. rustfmt
        // reflows a long literal and the leading whitespace it adds turns
        // `raw_source:` into a continuation of the `identifier` VALUE rather
        // than a key of its own - silently, and the test then fails against a
        // correct implementation.
        const TYPED_SOURCE_PAGE: &str = "---\ntype: Publication\nidentifier: Retracted Paper\nraw_source: [raw/retracted-paper/source.md]\n---\nbody";

        use crate::knowledge::raw::write_raw;
        use crate::knowledge::types::{Credibility, CredibilityTier, SourceMeta};

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");

        let meta = |id: &str, retracted: bool| SourceMeta {
            id: id.into(),
            title: format!("Title {id}"),
            url: Some("https://example.org/x".into()),
            ingested_at: chrono::Utc::now(),
            sha256: "abc".into(),
            mime: "text/html".into(),
            original_filename: Some("x.html".into()),
            credibility: Credibility {
                tier: CredibilityTier::PeerReviewed,
                confidence: 0.9,
                publisher: None,
                venue: None,
                doi: None,
                retracted,
                reasoning: "test".into(),
                classifier_version: 1,
            },
        };
        write_raw(&kb, None, None, "# r\n", meta("retracted-paper", true)).unwrap();

        // BioOKF's layout: a typed directory, and the anchor back to the raw
        // source. Nothing here is at `knowledge/sources/`.
        write_page(
            &kb,
            "knowledge/publication/retracted-paper.md",
            TYPED_SOURCE_PAGE,
            "add r",
            None,
        )
        .unwrap();

        let g = derive(&kb).unwrap();
        let node = g
            .nodes
            .iter()
            .find(|n| n.path == "knowledge/publication/retracted-paper.md")
            .expect("the typed source page must be a node");
        assert!(
            node.retracted,
            "a typed source page must inherit its retraction; without it the graph and \
             lint disagree about the same paper"
        );
        assert_eq!(
            node.credibility_tier,
            Some(CredibilityTier::PeerReviewed),
            "and its credibility tier, which is what the ring renders from"
        );
    }

    /// ⚠ **And a PLAIN OKF base, which is what `create_base` produces by
    /// default.** The test above is BioOKF-shaped, and the repair it was written
    /// for read exactly two signals — the `raw_source` anchor and the pre-OKF
    /// plural path — neither of which exists here. `raw_source` is a BioOKF-only
    /// key (`write_concept_spec` and `materialize_source_node`, its only two
    /// writers, are both gated on `KbFormat::is_biookf`), and OKF's directory is
    /// the SINGULAR `knowledge/source/`. So a retracted paper on an OKF base
    /// still drew as an ordinary node, and the pair of green tests either side of
    /// it said the layout bug was fixed.
    ///
    /// OKF states provenance as a `sources:` list whose `resource` points into
    /// `raw/` — `schema_okf.md`'s page contract — which is the spelling
    /// `source_anchor` had to learn.
    #[test]
    fn an_okf_source_page_inherits_its_retraction() {
        // ⚠ A const for the same reason the fixture above is one: rustfmt
        // reflows a long string literal, and the leading whitespace it inserts
        // turns a `sources:` entry into a continuation of the previous VALUE.
        const OKF_SOURCE_PAGE: &str = "---\ntype: Source\nidentifier: Retracted Paper\ntitle: Retracted Paper\nsources:\n  - id: retracted-paper\n    resource: raw/retracted-paper/source.md\n---\nbody";

        use crate::knowledge::affiliation::CallerAffiliation;
        use crate::knowledge::raw::write_raw;
        use crate::knowledge::types::{Credibility, CredibilityTier, KbFormat, SourceMeta};

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_as(
            "k",
            "K",
            None,
            KbFormat::Okf,
            false,
            &CallerAffiliation::Unstated,
        )
        .unwrap();
        let kb = dir.path().join("k");

        write_raw(
            &kb,
            None,
            None,
            "# r\n",
            SourceMeta {
                id: "retracted-paper".into(),
                title: "Retracted Paper".into(),
                url: Some("https://example.org/x".into()),
                ingested_at: chrono::Utc::now(),
                sha256: "abc".into(),
                mime: "text/html".into(),
                original_filename: Some("x.html".into()),
                credibility: Credibility {
                    tier: CredibilityTier::PeerReviewed,
                    confidence: 0.9,
                    publisher: None,
                    venue: None,
                    doi: None,
                    retracted: true,
                    reasoning: "test".into(),
                    classifier_version: 1,
                },
            },
        )
        .unwrap();

        // OKF's layout: the SINGULAR directory, and no `raw_source` key anywhere.
        write_page(
            &kb,
            "knowledge/source/retracted-paper.md",
            OKF_SOURCE_PAGE,
            "add r",
            None,
        )
        .unwrap();

        let g = derive(&kb).unwrap();
        let node = g
            .nodes
            .iter()
            .find(|n| n.path == "knowledge/source/retracted-paper.md")
            .expect("the OKF source page must be a node");
        assert!(
            node.retracted,
            "an OKF source page must inherit its retraction; without it the graph \
             and lint disagree about the same paper on the DEFAULT base format"
        );
        assert_eq!(
            node.credibility_tier,
            Some(CredibilityTier::PeerReviewed),
            "and its credibility tier, which is what the ring renders from"
        );
    }

    #[test]
    fn source_retracted_flag_propagates_to_graph_node() {
        use crate::knowledge::raw::write_raw;
        use crate::knowledge::types::{Credibility, CredibilityTier, SourceMeta};

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");

        // Helper: build a SourceMeta with a given retraction flag.
        let make_meta = |id: &str, retracted: bool| SourceMeta {
            id: id.into(),
            title: format!("Title {id}"),
            url: Some("https://example.org/x".into()),
            ingested_at: chrono::Utc::now(),
            sha256: "abc".into(),
            mime: "text/html".into(),
            original_filename: Some("x.html".into()),
            credibility: Credibility {
                tier: CredibilityTier::PeerReviewed,
                confidence: 0.9,
                publisher: None,
                venue: None,
                doi: None,
                retracted,
                reasoning: "test".into(),
                classifier_version: 1,
            },
        };

        // Two sources: one retracted, one not.
        write_raw(&kb, None, None, "# r\n", make_meta("retracted-paper", true)).unwrap();
        write_raw(&kb, None, None, "# ok\n", make_meta("ok-paper", false)).unwrap();
        // Source pages so the graph has corresponding nodes.
        write_page(
            &kb,
            "knowledge/sources/retracted-paper.md",
            "---\ntitle: Retracted Paper\nkind: source\n---\nbody",
            "add r",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/sources/ok-paper.md",
            "---\ntitle: OK Paper\nkind: source\n---\nbody",
            "add ok",
            None,
        )
        .unwrap();

        let g = derive(&kb).unwrap();
        let retracted_node = g
            .nodes
            .iter()
            .find(|n| n.id == "sources:retracted-paper")
            .expect("retracted source node");
        let ok_node = g
            .nodes
            .iter()
            .find(|n| n.id == "sources:ok-paper")
            .expect("ok source node");
        assert!(retracted_node.retracted, "retracted flag should propagate");
        assert!(!ok_node.retracted, "non-retracted source stays false");
    }

    // ── Stage 2: the typed graph ────────────────────────────────────────────

    /// One base from a list of `(logical path, file contents)`.
    fn kb_with(pages: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        for (path, content) in pages {
            write_page(&kb, path, content, "add", None).unwrap();
        }
        (dir, kb)
    }

    fn find_edge<'a>(g: &'a Graph, from: &str, to: &str) -> &'a GraphEdge {
        g.edges
            .iter()
            .find(|e| e.from == from && e.to == to)
            .unwrap_or_else(|| panic!("no {from} → {to} edge; edges={:?}", g.edges))
    }

    fn find_node<'a>(g: &'a Graph, id: &str) -> &'a GraphNode {
        g.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| {
            panic!(
                "no node {id}; nodes={:?}",
                g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            )
        })
    }

    /// BioOKF §12's worked example, trimmed to one edge that carries every
    /// attribute family at once: the provenance triplet, the §7.3 quantitative
    /// bundle (numeric *and* non-numeric slots), a §7.2 context attribute, a
    /// publication list and an attribute this build has never heard of.
    ///
    /// The last three are there so DR-27's split is exercised rather than
    /// asserted: `clinical_phase` is a statistic that is not a number,
    /// `species_context` is context that could be mistaken for one, and
    /// `assay_readout` is a key from no vocabulary this build knows.
    const TOCILIZUMAB: &str = "---
type: Molecule
identifier: Tocilizumab
subtype: antibody
status: draft
edges:
  - predicate: treats
    object: COVID-19
    knowledge_level: statistical_association
    agent_type: data_analysis_pipeline
    primary_source: RECOVERY trial
    effect_metric: relative_risk
    effect_size: 0.85
    ci_lower: 0.76
    ci_upper: 0.94
    p_value: 3.0e-6
    sample_size: 4116
    clinical_phase: approved
    species_context: human
    assay_readout: viral load
    publications: [PMID:33933206]
---

Blocks IL-6 signalling.
";

    const COVID: &str = "---
type: Disease
identifier: COVID-19
---

A respiratory illness.
";

    const RECOVERY: &str = "---
type: Study
identifier: RECOVERY trial
---

A platform trial.
";

    fn biookf_base() -> (tempfile::TempDir, std::path::PathBuf) {
        kb_with(&[
            ("knowledge/molecules/tocilizumab.md", TOCILIZUMAB),
            ("knowledge/concepts/covid-19.md", COVID),
            ("knowledge/sources/recovery-trial.md", RECOVERY),
        ])
    }

    /// The primary typed grammar (BioOKF §6). Every field asserted here is one
    /// the desktop edge inspector renders, so a deriver that resolves the target
    /// and drops the attributes looks correct on the canvas and is empty in the
    /// panel.
    #[test]
    fn a_frontmatter_edge_carries_its_predicate_provenance_and_statistics() {
        let (_d, kb) = biookf_base();
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "molecules:tocilizumab", "concepts:covid-19");
        assert_eq!(e.predicate.as_deref(), Some("treats"));
        assert_eq!(
            e.relation, e.predicate,
            "`relation` is the deprecated alias and must carry the same value"
        );
        assert!(!e.negated);
        assert_eq!(
            e.knowledge_level.as_deref(),
            Some("statistical_association")
        );
        assert_eq!(e.agent_type.as_deref(), Some("data_analysis_pipeline"));
        assert_eq!(e.publications, vec!["PMID:33933206".to_string()]);

        // DR-27: one open map, keyed by the slot the producer wrote. The six
        // flat fields this replaced could hold exactly these six keys and
        // silently dropped the other fourteen §7.3 names.
        assert_eq!(
            e.quantitative,
            [
                (
                    "effect_metric",
                    QuantitativeValue::Text("relative_risk".into())
                ),
                ("effect_size", QuantitativeValue::Number(0.85)),
                ("ci_lower", QuantitativeValue::Number(0.76)),
                ("ci_upper", QuantitativeValue::Number(0.94)),
                ("p_value", QuantitativeValue::Number(3.0e-6)),
                ("sample_size", QuantitativeValue::Number(4116.0)),
                // A §7.3 slot the old six fields had no room for, so it used to
                // land in `qualifiers` — the category error DR-27 names.
                ("clinical_phase", QuantitativeValue::Text("approved".into())),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
        );

        // …and the other map stays the other map. `species_context` is §7.2
        // context and belongs nowhere near the statistics; `assay_readout` is
        // from no vocabulary this build knows and must survive as text rather
        // than be dropped on the floor where nothing can report it.
        assert_eq!(
            e.qualifiers,
            [
                ("assay_readout", "viral load"),
                ("species_context", "human")
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        );
    }

    /// DR-24: `primary_source` names a source *node*, so it is emitted as a node
    /// id — the inspector's "show me that study" button has nothing to select
    /// otherwise.
    #[test]
    fn a_resolvable_primary_source_is_emitted_as_a_node_id() {
        let (_d, kb) = biookf_base();
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "molecules:tocilizumab", "concepts:covid-19");
        assert_eq!(e.primary_source.as_deref(), Some("sources:recovery-trial"));
    }

    #[test]
    fn an_unresolvable_primary_source_is_kept_exactly_as_written() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n    \
                 primary_source: A trial nobody wrote a page for\n---\nBody.\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "molecules:x", "concepts:covid-19");
        assert_eq!(
            e.primary_source.as_deref(),
            Some("A trial nobody wrote a page for"),
            "blanking it would leave lint nothing to name"
        );
    }

    #[test]
    fn nodes_carry_the_typed_frontmatter() {
        let (_d, kb) = biookf_base();
        let g = derive(&kb).unwrap();
        let n = find_node(&g, "molecules:tocilizumab");
        assert_eq!(n.node_type.as_deref(), Some("Molecule"));
        assert_eq!(n.subtype.as_deref(), Some("antibody"));
        assert_eq!(n.identifier.as_deref(), Some("Tocilizumab"));
        assert_eq!(n.status.as_deref(), Some("draft"));
        assert!(!n.stale);
        assert!(!n.external);
    }

    /// OKF §5.4 says an absent `status` *reads* as `stable`. Emitting it would
    /// make every legacy node claim a lifecycle stage its author never wrote.
    #[test]
    fn an_absent_status_is_absent_rather_than_defaulted_to_stable() {
        let (_d, kb) = biookf_base();
        let g = derive(&kb).unwrap();
        assert!(find_node(&g, "concepts:covid-19").status.is_none());
    }

    #[test]
    fn a_stale_after_in_the_past_marks_the_node_stale() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/concepts/old.md",
                "---\ntype: Concept\nidentifier: Old\nstale_after: 2001-01-01\n---\nBody.\n",
            ),
            (
                "knowledge/concepts/fresh.md",
                "---\ntype: Concept\nidentifier: Fresh\nstale_after: 2999-01-01\n---\nBody.\n",
            ),
        ]);
        let g = derive(&kb).unwrap();
        assert!(find_node(&g, "concepts:old").stale);
        assert!(!find_node(&g, "concepts:fresh").stale);
    }

    /// DR-2's second grammar. `is_bundle_link` is the half that matters here: a
    /// KB page cites papers, and an edge to `https://example.org` on every page
    /// is the fastest way to make a graph useless.
    #[test]
    fn an_okf_markdown_link_is_an_untyped_edge_and_a_web_link_is_not() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/concepts/il6.md",
                "---\ntitle: IL6\n---\nSee [COVID-19](/knowledge/concepts/covid-19.md), \
                 reported in [the trial](https://example.org/recovery).\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "concepts:il6", "concepts:covid-19");
        assert!(
            e.predicate.is_none(),
            "OKF §6.1 calls a markdown link an *untyped* edge"
        );
        assert_eq!(
            g.edges.len(),
            1,
            "the https link must not become an edge: {:?}",
            g.edges
        );
    }

    /// DR-2's third grammar, and the one whose whole point is the attributes.
    #[test]
    fn inline_edge_sugar_is_a_typed_edge_carrying_its_attributes() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\n---\n\
                 It [[treats:: COVID-19 | knowledge_level=knowledge_assertion; \
                 agent_type=manual_agent; primary_source=RECOVERY trial]].\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
            ("knowledge/sources/recovery-trial.md", RECOVERY),
        ]);
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "molecules:x", "concepts:covid-19");
        assert_eq!(e.predicate.as_deref(), Some("treats"));
        assert_eq!(e.knowledge_level.as_deref(), Some("knowledge_assertion"));
        assert_eq!(e.agent_type.as_deref(), Some("manual_agent"));
        assert_eq!(e.primary_source.as_deref(), Some("sources:recovery-trial"));
    }

    /// DR-25. Without the absorption a BioOKF page renders every relationship
    /// twice — once typed, once grey — which reads as a rendering bug and is a
    /// deriver bug.
    #[test]
    fn a_typed_edge_absorbs_the_prose_link_that_restates_it() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n---\n\
                 X treats [[COVID-19]], see also [it](/knowledge/concepts/covid-19.md).\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        assert_eq!(
            g.edges.len(),
            1,
            "one relationship written three ways is one edge: {:?}",
            g.edges
        );
        assert_eq!(g.edges[0].predicate.as_deref(), Some("treats"));
    }

    #[test]
    fn two_predicates_between_the_same_pair_are_two_edges() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n  \
                 - predicate: associated_with\n    object: COVID-19\n  \
                 - predicate: treats\n    object: COVID-19\n---\nBody.\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        let mut predicates: Vec<&str> = g
            .edges
            .iter()
            .filter_map(|e| e.predicate.as_deref())
            .collect();
        predicates.sort_unstable();
        assert_eq!(
            predicates,
            vec!["associated_with", "treats"],
            "the key is (from, to, predicate), so the repeat collapses and the \
             second predicate does not: {:?}",
            g.edges
        );
    }

    /// SPEC §6.F, both spellings. A renderer that only knows the `not_` prefix
    /// draws a `negated: true` claim as a positive one.
    #[test]
    fn both_spellings_of_a_negative_claim_come_back_negated() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: not_treats\n    object: COVID-19\n---\nBody.\n",
            ),
            (
                "knowledge/molecules/y.md",
                "---\ntype: Molecule\nidentifier: Y\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n    negated: true\n---\nBody.\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        assert!(find_edge(&g, "molecules:x", "concepts:covid-19").negated);
        assert!(find_edge(&g, "molecules:y", "concepts:covid-19").negated);
    }

    /// A statistic written the way a model actually writes it. Parsing it to
    /// `None` would delete it with nothing left to report the loss.
    ///
    /// **And it stays in the same map as its numeric twin**, which is the half
    /// this pins. The obvious implementation of DR-27 splits the two maps on the
    /// *value*, and then `p_value: 3.0e-6` (the fixture above) and
    /// `p_value: <0.001` (here) are one slot filed in two places, so a consumer
    /// looking for a p-value has to look in both and merge them.
    #[test]
    fn an_unparseable_statistic_stays_quantitative_as_text() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n    p_value: \"<0.001\"\n\
                     \x20   effect_size: 2.5 nM\n---\nBody.\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "molecules:x", "concepts:covid-19");
        assert_eq!(
            e.quantitative.get("p_value"),
            Some(&QuantitativeValue::Text("<0.001".into()))
        );
        assert_eq!(
            e.quantitative.get("effect_size"),
            Some(&QuantitativeValue::Text("2.5 nM".into()))
        );
        assert!(
            e.qualifiers.is_empty(),
            "a statistic must not fall through to the context map: {:?}",
            e.qualifiers
        );
    }

    /// `NaN`, `inf` and `infinity` all parse as `f64` and none of them can be
    /// serialized to JSON. Admitting one as a number would fail the graph cache
    /// write for the whole base over a single attribute on a single edge — so
    /// they are text, and the base still writes.
    #[test]
    fn a_non_finite_statistic_is_text_so_the_cache_can_still_be_written() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n    effect_size: NaN\n\
                     \x20   p_value: inf\n---\nBody.\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        let e = find_edge(&g, "molecules:x", "concepts:covid-19");
        assert_eq!(
            e.quantitative.get("effect_size"),
            Some(&QuantitativeValue::Text("NaN".into()))
        );
        assert_eq!(
            e.quantitative.get("p_value"),
            Some(&QuantitativeValue::Text("inf".into()))
        );
        write_cache(&kb, &g).expect("a non-finite statistic must not cost the cache");
    }

    /// UI spec §2.1's `degree`, which §5.6 sizes hubs by and §5.12 orders the
    /// keyboard walk by. Counted per incident edge at each end, **after** DR-25's
    /// deduplication — a node whose page happens to spell one relationship in two
    /// grammars is not a bigger hub for it.
    #[test]
    fn degree_counts_incident_edges_at_both_ends_after_deduplication() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                // The same relationship twice: typed in `edges:`, restated as
                // prose. DR-25 collapses it to one edge.
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n---\n\
                 Also see [[COVID-19]].\n",
            ),
            (
                "knowledge/molecules/y.md",
                "---\ntype: Molecule\nidentifier: Y\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n---\nBody.\n",
            ),
            ("knowledge/concepts/covid-19.md", COVID),
            (
                "knowledge/concepts/lonely.md",
                "---\ntitle: Lonely\n---\n.\n",
            ),
        ]);
        let g = derive(&kb).unwrap();
        assert_eq!(g.edges.len(), 2, "edges={:?}", g.edges);
        assert_eq!(find_node(&g, "molecules:x").degree, Some(1));
        assert_eq!(find_node(&g, "molecules:y").degree, Some(1));
        assert_eq!(
            find_node(&g, "concepts:covid-19").degree,
            Some(2),
            "both ends are counted"
        );
        assert_eq!(
            find_node(&g, "concepts:lonely").degree,
            Some(0),
            "counted and zero, not absent — see GraphNode::degree"
        );
    }

    // ── Dangling targets (requirement D) ────────────────────────────────────

    #[test]
    fn a_dangling_typed_edge_becomes_an_external_node() {
        let (_d, kb) = kb_with(&[(
            "knowledge/molecules/x.md",
            "---\ntype: Molecule\nidentifier: X\nedges:\n  \
             - predicate: has_phenotype\n    object: Neutropenia\n---\nBody.\n",
        )]);
        let g = derive(&kb).unwrap();
        let n = find_node(&g, "external::neutropenia");
        assert!(n.external);
        assert_eq!(n.label, "Neutropenia");
        assert_eq!(n.identifier.as_deref(), Some("Neutropenia"));
        assert_eq!(n.path, "", "there is no page to open");
        assert_eq!(
            find_edge(&g, "molecules:x", "external::neutropenia")
                .predicate
                .as_deref(),
            Some("has_phenotype")
        );
    }

    /// The gate measured this one drawing a **file path** in the graph.
    ///
    /// A markdown link points at a file and *calls* the target something; the
    /// deriver used the destination for both, so the node arrived labelled
    /// `/knowledge/concepts/nonexistent-thing.md`. The identity still comes from
    /// the destination — that is what two pages must agree on — and only the
    /// display name comes from the text.
    #[test]
    fn a_dangling_markdown_link_is_named_by_its_text_and_not_by_its_path() {
        let (_d, kb) = kb_with(&[(
            "knowledge/concepts/a.md",
            "---\ntitle: A\n---\n\
             See [Nonexistent Thing](/knowledge/concepts/nonexistent-thing.md).\n",
        )]);
        let g = derive(&kb).unwrap();
        let n = find_node(&g, "external::nonexistent-thing");
        assert!(n.external);
        assert_eq!(n.label, "Nonexistent Thing");
        assert_eq!(n.identifier.as_deref(), Some("Nonexistent Thing"));
    }

    /// No link text to use, so the basename is humanised — separators to spaces,
    /// `.md` off, and the case left exactly as written.
    #[test]
    fn a_dangling_link_with_no_text_is_named_by_its_humanised_basename() {
        let (_d, kb) = kb_with(&[(
            "knowledge/concepts/a.md",
            "---\ntitle: A\n---\nSee [](/knowledge/concepts/nonexistent-thing.md).\n",
        )]);
        let g = derive(&kb).unwrap();
        assert_eq!(
            find_node(&g, "external::nonexistent-thing").label,
            "nonexistent thing"
        );
    }

    /// The guard on the humanising, and the reason it is conditional. A target
    /// that is *not* path-shaped is already the name its author chose, and `-`
    /// in `COVID-19` is part of that name — an unconditional separator swap
    /// would rewrite far more names than it repaired.
    #[test]
    fn a_hyphen_in_a_bare_identifier_is_part_of_the_name_and_survives() {
        let (_d, kb) = kb_with(&[(
            "knowledge/molecules/x.md",
            "---\ntype: Molecule\nidentifier: X\nedges:\n  \
             - predicate: treats\n    object: COVID-19\n---\nBody.\n",
        )]);
        let g = derive(&kb).unwrap();
        assert_eq!(find_node(&g, "external::covid-19").label, "COVID-19");
    }

    #[test]
    fn one_missing_concept_named_by_two_pages_is_one_external_node() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: has_phenotype\n    object: Neutropenia\n---\nBody.\n",
            ),
            (
                "knowledge/molecules/y.md",
                "---\ntype: Molecule\nidentifier: Y\n---\n\
                 Also [[causes:: neutropenia]] sometimes.\n",
            ),
            // A third spelling, and the one that made the naming fix delicate:
            // it points at a *path*. Identity must still come from the target,
            // or naming the node after the link text would split it in two.
            (
                "knowledge/molecules/z.md",
                "---\ntitle: Z\n---\n\
                 And [Neutropenia](/knowledge/concepts/neutropenia.md) too.\n",
            ),
        ]);
        let g = derive(&kb).unwrap();
        assert_eq!(
            g.nodes.iter().filter(|n| n.external).count(),
            1,
            "three spellings of one missing concept must share a placeholder: {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        assert_eq!(g.edges.len(), 3);
    }

    /// The back-compat half of requirement D, and the reason the recording is
    /// keyed on the *grammar*. A legacy base is bracket-links-only, so recording
    /// its dangling links would change its node set — and `links.rs`'s
    /// equivalence test says in as many words that a dangling link must not
    /// invent a node.
    #[test]
    fn a_dangling_legacy_bracket_link_still_draws_nothing() {
        let (_d, kb) = kb_with(&[(
            "knowledge/concepts/a.md",
            "---\ntitle: A\n---\nPoints at [[Nowhere At All]].\n",
        )]);
        let g = derive(&kb).unwrap();
        assert!(g.edges.is_empty(), "edges={:?}", g.edges);
        assert_eq!(g.nodes.len(), 1, "no placeholder invented: {:?}", g.nodes);
    }

    #[test]
    fn a_link_to_a_missing_scaffold_page_invents_nothing() {
        // The index page is deliberately not a node; a markdown link to it must
        // not resurrect it as a placeholder hub connected to everything.
        let (_d, kb) = kb_with(&[(
            "knowledge/concepts/a.md",
            "---\ntitle: A\n---\nBack to [the index](/knowledge/index.md).\n",
        )]);
        let g = derive(&kb).unwrap();
        assert!(g.nodes.iter().all(|n| !n.external), "{:?}", g.nodes);
        assert!(g.edges.is_empty());
    }

    // ── DR-3's identity ladder (requirement E) ──────────────────────────────

    /// The whole point of the ladder: a page renamed away from the stem its
    /// inbound edges were written against keeps its `identifier`, so the edges
    /// still find it.
    #[test]
    fn an_inbound_edge_survives_a_rename_through_the_identifier_rung() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/molecules/x.md",
                "---\ntype: Molecule\nidentifier: X\nedges:\n  \
                 - predicate: treats\n    object: COVID-19\n---\nBody.\n",
            ),
            // Renamed from `covid-19.md`; the identifier did not move.
            ("knowledge/concepts/sars-cov-2-infection.md", COVID),
        ]);
        let g = derive(&kb).unwrap();
        find_edge(&g, "molecules:x", "concepts:sars-cov-2-infection");
    }

    #[test]
    fn the_title_rung_resolves_a_target_the_basename_rung_cannot() {
        // SPEC §14 makes `title` the deprecated alias of `identifier`, so a page
        // that only has one is still findable by the name it shows.
        let (_d, kb) = kb_with(&[
            (
                "knowledge/concepts/a.md",
                "---\ntitle: A\n---\nSee [[Zone-2 base]].\n",
            ),
            (
                "knowledge/concepts/z2.md",
                "---\ntitle: Zone-2 base\n---\nBody.\n",
            ),
        ]);
        let g = derive(&kb).unwrap();
        find_edge(&g, "concepts:a", "concepts:z2");
    }

    #[test]
    fn the_basename_rung_still_resolves_a_page_with_no_frontmatter_at_all() {
        let (_d, kb) = kb_with(&[
            ("knowledge/concepts/a.md", "See [[bare-page]].\n"),
            ("knowledge/concepts/bare-page.md", "Just prose.\n"),
        ]);
        let g = derive(&kb).unwrap();
        find_edge(&g, "concepts:a", "concepts:bare-page");
    }

    // ── The property that governs the stage ─────────────────────────────────

    /// A legacy base derives exactly the graph it always did, and — the part a
    /// field-by-field assertion would miss — its JSON gains **two** keys and no
    /// others.
    ///
    /// The prose here used to say it gained none, which was never what the code
    /// did: the allowed set below has listed `identifier` since Stage 2, on
    /// purpose and with a reason. `degree` joined it at DR-27. Neither is new
    /// *information* — `identifier` is SPEC §14's deprecated alias for the
    /// `title` these pages already carry, and `degree` is a count of edges that
    /// were already in the payload — but "no keys" and "two keys, both derived
    /// from what was already there" are different claims, and a comment that
    /// overstates what the code does is worse than no comment: the next reader
    /// finds `identifier` in the list, believes the prose, and deletes it.
    ///
    /// Every other new field is skipped when absent, which is the property that
    /// actually holds.
    #[test]
    fn a_legacy_base_derives_exactly_what_it_always_did() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 2);
        assert!(g
            .nodes
            .iter()
            .all(|n| !n.external && !n.stale && n.node_type.is_none() && n.subtype.is_none()));
        assert!(g
            .edges
            .iter()
            .all(|e| e.relation.is_none() && e.predicate.is_none() && !e.negated));

        let json = serde_json::to_value(&g).unwrap();
        // The two additions, both derived from what the base already held:
        // `identifier` because these pages carry `title`, which SPEC §14 makes
        // its deprecated alias, so the display identity was already on disk; and
        // `degree` because it counts the edges in this same payload.
        let allowed: HashSet<&str> = [
            "id",
            "label",
            "kind",
            "retracted",
            "path",
            "identifier",
            "degree",
        ]
        .into_iter()
        .collect();
        for node in json["nodes"].as_array().unwrap() {
            let keys: Vec<&str> = node
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert!(
                keys.iter().all(|k| allowed.contains(k)),
                "a legacy node grew a key: {keys:?}"
            );
        }
        for edge in json["edges"].as_array().unwrap() {
            let keys: Vec<&str> = edge
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, vec!["from", "to"], "a legacy edge grew a key");
        }
    }

    /// The pre-Stage-2 deriver scanned the whole file, frontmatter included, so
    /// a bracket link written into a `description:` produced an edge. Narrowing
    /// that to the body would silently delete edges from bases on disk — while a
    /// markdown link is newly read, and YAML is full of near-misses for one.
    #[test]
    fn brackets_are_read_from_the_frontmatter_and_markdown_links_are_not() {
        let (_d, kb) = kb_with(&[
            (
                "knowledge/concepts/a.md",
                "---\ntitle: A\ndescription: \"like [[b]], unlike [c](/knowledge/concepts/c.md)\"\n\
                 ---\nBody.\n",
            ),
            ("knowledge/concepts/b.md", "---\ntitle: B\n---\nBody.\n"),
            ("knowledge/concepts/c.md", "---\ntitle: C\n---\nBody.\n"),
        ]);
        let g = derive(&kb).unwrap();
        assert_eq!(
            g.edges.iter().map(|e| e.to.as_str()).collect::<Vec<_>>(),
            vec!["concepts:b"],
            "edges={:?}",
            g.edges
        );
    }

    /// Requirement F, and the reason DR-13 landed the envelope a stage early. A
    /// v1 cache holds nodes with no `node_type`; every new field is
    /// `#[serde(default)]`, so it would deserialize *cleanly* and serve typeless
    /// nodes for ever. Only the version stops it.
    #[test]
    fn a_v1_cache_from_before_the_typed_graph_is_absent_and_re_derived() {
        let (_d, kb) = biookf_base();
        let typeless = serde_json::json!({
            "version": 1,
            "graph": {
                "nodes": [{
                    "id": "molecules:tocilizumab", "label": "tocilizumab",
                    "kind": "hub", "retracted": false,
                    "path": "knowledge/molecules/tocilizumab.md",
                }],
                "edges": [{"from": "molecules:tocilizumab", "to": "concepts:covid-19"}],
            },
        });
        overwrite_cache_file(&kb, &typeless.to_string());
        assert!(
            read_cache(&kb)
                .expect("a stale cache is never an error")
                .is_none(),
            "a v1 cache must read as absent, not as a typeless graph"
        );

        let svc = KnowledgeService::new(_d.path().to_path_buf());
        let g = svc.get_graph("k").unwrap();
        assert_eq!(
            find_node(&g, "molecules:tocilizumab").node_type.as_deref(),
            Some("Molecule"),
            "the base re-derived and is typed from here on"
        );
        assert_eq!(read_cache(&kb).unwrap().as_ref(), Some(&g), "cache healed");
    }

    /// The same hazard one generation on, and the quieter of the two.
    ///
    /// A v2 cache is *typed* — it deserializes into today's `Graph` with nothing
    /// visibly missing — but its edges carry the six flat statistical keys under
    /// names nothing reads any more, and not one of its nodes has a `degree`. A
    /// renderer sizing hubs by `degree` would then draw every node in every
    /// pre-existing base at the same size, which reads as a layout bug and not
    /// as a stale cache. Only the version number separates the two.
    #[test]
    fn a_v2_cache_from_before_the_open_quantitative_map_is_absent_and_re_derived() {
        let (_d, kb) = biookf_base();
        let flat_stats = serde_json::json!({
            "version": 2,
            "graph": {
                "nodes": [{
                    "id": "molecules:tocilizumab", "label": "tocilizumab",
                    "kind": "hub", "retracted": false,
                    "path": "knowledge/molecules/tocilizumab.md",
                    "node_type": "Molecule", "identifier": "Tocilizumab",
                }],
                "edges": [{
                    "from": "molecules:tocilizumab", "to": "concepts:covid-19",
                    "predicate": "treats", "p_value": 3.0e-6, "effect_size": 0.85,
                }],
            },
        });
        overwrite_cache_file(&kb, &flat_stats.to_string());
        assert!(
            read_cache(&kb)
                .expect("a stale cache is never an error")
                .is_none(),
            "a v2 cache must read as absent, not as a degreeless typed graph"
        );

        let svc = KnowledgeService::new(_d.path().to_path_buf());
        let g = svc.get_graph("k").unwrap();
        assert!(
            find_node(&g, "molecules:tocilizumab").degree.is_some(),
            "the base re-derived and carries degrees from here on"
        );
        assert!(!find_edge(&g, "molecules:tocilizumab", "concepts:covid-19")
            .quantitative
            .is_empty());
    }
}
