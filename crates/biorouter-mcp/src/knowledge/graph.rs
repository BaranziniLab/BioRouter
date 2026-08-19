use crate::knowledge::{
    links::{self, LinkIndex},
    raw,
    store::{self, PageRef},
    types::{Graph, GraphEdge, GraphNode, PageKind},
};
use anyhow::Result;
use std::path::Path;

pub fn derive(kb_root: &Path) -> Result<Graph> {
    // `list_pages` walks the whole `knowledge/` tree, which includes the
    // scaffold pages `index.md` and `log.md`. The index page links to (or is
    // linked from) virtually every other page, so as a graph node it becomes a
    // giant hub connected to everything — visually redundant and it crowds out
    // the real structure. Exclude scaffold pages from the graph entirely; any
    // `[[…]]` link pointing at them is then silently dropped because they never
    // enter `label_to_id`.
    let pages: Vec<PageRef> = store::list_pages(kb_root, None)?
        .into_iter()
        .filter(|p| !is_scaffold_page(&p.path))
        .collect();
    let mut nodes = Vec::new();
    let mut id_for_path = std::collections::HashMap::new();
    let mut by_link_key = Vec::new();

    for p in &pages {
        let node_id = path_to_node_id(&p.path);
        id_for_path.insert(p.path.clone(), node_id.clone());
        by_link_key.push((p.path.clone(), node_id.clone()));
        let kind = page_kind_of(p);
        nodes.push(GraphNode {
            id: node_id,
            label: p.title.clone(),
            kind,
            credibility_tier: None,
            retracted: false,
            path: p.path.clone(),
        });
    }

    // Source nodes inherit credibility from raw/<id>/meta.yaml.
    for src in raw::list_sources(kb_root)? {
        let logical = format!("knowledge/sources/{}.md", src.id);
        if let Some(node_id) = id_for_path.get(&logical) {
            if let Some(n) = nodes.iter_mut().find(|n| &n.id == node_id) {
                n.credibility_tier = Some(src.credibility.tier);
                n.retracted = src.credibility.retracted;
            }
        }
    }

    // One index, one keying, shared with the lint and the query citation
    // extractor — see `knowledge::links` for the three that used to disagree.
    // A link to a scaffold page resolves to nothing, because the scaffold pages
    // were filtered out above and so never entered the index.
    let index: LinkIndex<String> = LinkIndex::from_pages(by_link_key);
    let mut edges = Vec::new();
    for p in &pages {
        let abs = kb_root.join(&p.path);
        let body = std::fs::read_to_string(&abs)?;
        let from = id_for_path.get(&p.path).cloned().unwrap();
        for link in links::wiki_links(&body) {
            // A self-link is dropped rather than drawn: a page that mentions
            // its own name would otherwise get a loop that says nothing.
            if let Some(to) = index.resolve(&link.target).filter(|to| *to != &from) {
                edges.push(GraphEdge {
                    from: from.clone(),
                    to: to.clone(),
                    // Stage 2's socket. `link.predicate` is already carried by
                    // the BioOKF sugar form and is deliberately dropped here:
                    // populating it is a graph-shape change with its own gate,
                    // and this seam must not smuggle one in.
                    relation: None,
                });
            }
        }
    }

    Ok(Graph { nodes, edges })
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
const CACHE_VERSION: u32 = 1;

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
/// nodes: the auto-maintained index and the change log. Matched on the logical
/// path so a real page that merely contains the word "index" is unaffected.
fn is_scaffold_page(logical: &str) -> bool {
    matches!(
        logical,
        "knowledge/index.md" | "knowledge/log.md" | "index.md" | "log.md"
    )
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
            "knowledge/sources/wanjun-gu---google-scholar-d7f205.md",
            "---\ntitle: Wanjun Gu, Google Scholar (URL reference)\nkind: source\n---\n\
             References [[knowledge/entities/wanjun-gu|Wanjun Gu]] and\n\
             also [[knowledge/entities/wanjun-gu.md|Wanjun Gu]] (with .md).\n",
            "add source",
            None,
        )
        .unwrap();

        let g = derive(&kb).unwrap();
        assert_eq!(g.nodes.len(), 2);
        // Both piped wiki-link forms should resolve to the entity page.
        let edges_to_entity = g
            .edges
            .iter()
            .filter(|e| e.to == "entities:wanjun-gu")
            .count();
        assert!(
            edges_to_entity >= 2,
            "expected piped wiki-links to produce edges, got edges: {:?}",
            g.edges
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
}
