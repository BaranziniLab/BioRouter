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
    types::{Graph, GraphEdge, GraphNode, PageKind},
};
use anyhow::Result;
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub fn derive(kb_root: &Path) -> Result<Graph> {
    let pages = load_pages(kb_root)?;
    // One date for the whole derivation, so two nodes with the same
    // `stale_after` can never disagree because the clock ticked between them.
    let today = chrono::Utc::now().date_naive();
    let mut nodes: Vec<GraphNode> = pages.iter().map(|p| node_for(p, today)).collect();
    apply_source_credibility(kb_root, &mut nodes)?;

    let index = NodeIndex::build(&pages);
    let mut collector = EdgeCollector::default();
    for page in &pages {
        collector.collect_page(page, &index);
    }
    // Externals last: a real page always outranks a placeholder for the same
    // concept, and appending keeps every existing node's position unchanged.
    nodes.extend(collector.external.into_values());
    Ok(Graph {
        nodes,
        edges: dedupe(collector.edges),
    })
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

fn load_pages(kb_root: &Path) -> Result<Vec<LoadedPage>> {
    // `list_pages` walks the whole `knowledge/` tree, which includes the
    // scaffold pages `index.md` and `log.md`. The index page links to (or is
    // linked from) virtually every other page, so as a graph node it becomes a
    // giant hub connected to everything — visually redundant and it crowds out
    // the real structure. Exclude scaffold pages from the graph entirely; any
    // link pointing at them then resolves to nothing.
    let mut out = Vec::new();
    for page in store::list_pages(kb_root, None)? {
        if is_scaffold_page(&page.path) {
            continue;
        }
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
    }
}

/// Source nodes inherit credibility from `raw/<id>/meta.yaml`.
fn apply_source_credibility(kb_root: &Path, nodes: &mut [GraphNode]) -> Result<()> {
    for src in raw::list_sources(kb_root)? {
        let logical = format!("knowledge/sources/{}.md", src.id);
        if let Some(n) = nodes.iter_mut().find(|n| n.path == logical) {
            n.credibility_tier = Some(src.credibility.tier);
            n.retracted = src.credibility.retracted;
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
    fn build(pages: &[LoadedPage]) -> Self {
        let mut by_identifier = HashMap::new();
        let mut by_title = HashMap::new();
        for p in pages {
            insert_name(&mut by_identifier, p.doc.identifier.as_deref(), &p.node_id);
            insert_name(&mut by_title, p.doc.title.as_deref(), &p.node_id);
        }
        Self {
            by_identifier,
            by_title,
            by_basename: LinkIndex::from_pages(
                pages
                    .iter()
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

/// Where an edge points, once the ladder has had its say.
enum Target {
    /// A page in this bundle.
    Page(String),
    /// No page yet: the node id to draw a placeholder under.
    External(String),
    /// Nothing to draw — a scaffold page, an off-bundle URI, or a self-link. A
    /// page that mentions its own name would otherwise get a loop that says
    /// nothing.
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
}

impl EdgeCollector {
    fn collect_page(&mut self, p: &LoadedPage, index: &NodeIndex) {
        // BioOKF §6 first, because DR-25's tie-break is first-wins: a typed
        // entry has to reach `dedupe` before the prose link that restates it.
        for e in &p.doc.edges {
            self.push(
                index,
                &p.node_id,
                &e.object,
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
                &link.target,
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
                    &link.target,
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
        written: &str,
        predicate: Option<String>,
        attrs: EdgeAttrs,
        record_dangling: bool,
    ) {
        let to = match classify_target(index, from, written, record_dangling) {
            Target::Page(id) => id,
            Target::External(id) => {
                self.ensure_external(&id, written);
                id
            }
            Target::Drop => return,
        };
        self.edges
            .push(make_edge(from, to, predicate, attrs, index));
    }

    fn ensure_external(&mut self, id: &str, written: &str) {
        let name = written.trim();
        self.external
            .entry(id.to_string())
            .or_insert_with(|| GraphNode {
                id: id.to_string(),
                label: name.to_string(),
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
                identifier: Some(name.to_string()),
                status: None,
                stale: false,
                external: true,
            });
    }
}

fn classify_target(index: &NodeIndex, from: &str, written: &str, record_dangling: bool) -> Target {
    match index.resolve(written) {
        Some(to) if to == from => Target::Drop,
        Some(to) => Target::Page(to.clone()),
        // The scaffold check sits on the *dangling* branch alone, on purpose. A
        // real page at `knowledge/topics/index.md` is a node, and a link naming
        // it resolves above; checking the name first would drop that edge.
        None if record_dangling && !is_scaffold_target(written) => {
            let key = links::link_key(written);
            if key.is_empty() {
                Target::Drop
            } else {
                // `external::` cannot collide with a page-derived id: those are
                // path segments joined by a single `:`, and a walked directory
                // never yields an empty segment.
                Target::External(format!("external::{key}"))
            }
        }
        None => Target::Drop,
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
        effect_metric: attrs.effect_metric,
        effect_size: attrs.effect_size,
        ci_lower: attrs.ci_lower,
        ci_upper: attrs.ci_upper,
        p_value: attrs.p_value,
        sample_size: attrs.sample_size,
        qualifiers: attrs.qualifiers,
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
    effect_metric: Option<String>,
    effect_size: Option<f64>,
    ci_lower: Option<f64>,
    ci_upper: Option<f64>,
    p_value: Option<f64>,
    sample_size: Option<u64>,
    qualifiers: BTreeMap<String, String>,
}

impl EdgeAttrs {
    fn absorb(&mut self, key: &str, value: &str) {
        let v = value.trim();
        if v.is_empty() {
            return;
        }
        if self.absorb_named(key, v) || self.absorb_number(key, v) {
            return;
        }
        self.qualifiers.insert(key.to_string(), v.to_string());
    }

    fn absorb_named(&mut self, key: &str, v: &str) -> bool {
        match key {
            "knowledge_level" => self.knowledge_level = Some(v.to_string()),
            "agent_type" => self.agent_type = Some(v.to_string()),
            "primary_source" => self.primary_source = Some(v.to_string()),
            "effect_metric" => self.effect_metric = Some(v.to_string()),
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

    /// Returns false — so [`Self::absorb`] falls through to `qualifiers` — when
    /// a quantitative key does not hold a number. `p_value: <0.001` and
    /// `effect_size: 2.5 nM` are both things a model writes, and parsing them to
    /// `None` deletes a statistic with nothing left to report it.
    fn absorb_number(&mut self, key: &str, v: &str) -> bool {
        let slot: &mut Option<f64> = match key {
            "effect_size" => &mut self.effect_size,
            "ci_lower" => &mut self.ci_lower,
            "ci_upper" => &mut self.ci_upper,
            "p_value" => &mut self.p_value,
            "sample_size" => {
                self.sample_size = v.parse().ok();
                return self.sample_size.is_some();
            }
            _ => return false,
        };
        *slot = v.parse().ok();
        slot.is_some()
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
const CACHE_VERSION: u32 = 2;

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

pub fn write_cache(kb_root: &Path, graph: &Graph) -> Result<()> {
    let path = kb_root
        .join(".biorouter-knowledge")
        .join("graph-cache.json");
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
    let path = kb_root
        .join(".biorouter-knowledge")
        .join("graph-cache.json");
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
fn is_scaffold_page(logical: &str) -> bool {
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
        let path = kb.join(".biorouter-knowledge").join("graph-cache.json");
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
        let raw = std::fs::read_to_string(kb.join(".biorouter-knowledge").join("graph-cache.json"))
            .unwrap();
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
    /// bundle, a publication list and an attribute this build does not model.
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
        assert_eq!(e.effect_metric.as_deref(), Some("relative_risk"));
        assert_eq!(e.effect_size, Some(0.85));
        assert_eq!(e.ci_lower, Some(0.76));
        assert_eq!(e.ci_upper, Some(0.94));
        assert_eq!(e.p_value, Some(3.0e-6));
        assert_eq!(e.sample_size, Some(4116));
        assert_eq!(
            e.qualifiers.get("clinical_phase").map(String::as_str),
            Some("approved"),
            "an attribute this build does not model must survive as text, not \
             be dropped: {:?}",
            e.qualifiers
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
    #[test]
    fn an_unparseable_statistic_survives_as_a_qualifier() {
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
        assert_eq!(e.p_value, None);
        assert_eq!(
            e.qualifiers.get("p_value").map(String::as_str),
            Some("<0.001")
        );
        assert_eq!(
            e.qualifiers.get("effect_size").map(String::as_str),
            Some("2.5 nM")
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
        ]);
        let g = derive(&kb).unwrap();
        assert_eq!(
            g.nodes.iter().filter(|n| n.external).count(),
            1,
            "two spellings of one missing concept must share a placeholder: {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        assert_eq!(g.edges.len(), 2);
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
    /// field-by-field assertion would miss — its JSON gains no keys. Every new
    /// field is skipped when absent, so a base of untyped pages serializes to
    /// what this build produced before Stage 2.
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
        // `identifier` is the one addition, and it is not new information: these
        // pages carry `title`, which SPEC §14 makes the deprecated alias of
        // `identifier`, so the display identity was already on disk.
        let allowed: HashSet<&str> = ["id", "label", "kind", "retracted", "path", "identifier"]
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
}
