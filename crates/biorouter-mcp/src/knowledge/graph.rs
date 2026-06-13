use crate::knowledge::{
    raw,
    store::{self, PageRef},
    types::{Graph, GraphEdge, GraphNode, PageKind},
};
use anyhow::Result;
use std::path::Path;

const KNOWLEDGE_LINK_RE: &str = r"\[\[([^\]]+)\]\]";

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
    let mut label_to_id = std::collections::HashMap::new();

    for p in &pages {
        let node_id = path_to_node_id(&p.path);
        id_for_path.insert(p.path.clone(), node_id.clone());
        label_to_id.insert(slug(page_basename(&p.path)).to_lowercase(), node_id.clone());
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

    let mut edges = Vec::new();
    let re = regex::Regex::new(KNOWLEDGE_LINK_RE).unwrap();
    for p in &pages {
        let abs = kb_root.join(&p.path);
        let body = std::fs::read_to_string(&abs)?;
        let from = id_for_path.get(&p.path).cloned().unwrap();
        for cap in re.captures_iter(&body) {
            let raw = cap.get(1).unwrap().as_str().trim();
            // Support Obsidian-style `[[target|alias]]` wiki-link syntax: only
            // the part before the first `|` is the link target; the alias is
            // display text. Without this split, the slug of the full bracket
            // contents never matches any page's basename and every link is
            // silently dropped.
            let target = raw.split('|').next().unwrap_or(raw).trim();
            // Targets may be plain labels (e.g. "Wanjun Gu") or full logical
            // paths (e.g. "knowledge/entities/wanjun-gu" or with `.md`). For
            // path-style targets, only the file basename participates in the
            // label lookup since `label_to_id` is keyed by `slug(basename)`.
            let basename = target
                .rsplit('/')
                .next()
                .unwrap_or(target)
                .trim_end_matches(".md");
            let key = slug(&basename.to_lowercase());
            if let Some(to) = label_to_id.get(&key) {
                if to != &from {
                    edges.push(GraphEdge {
                        from: from.clone(),
                        to: to.clone(),
                        relation: None,
                    });
                }
            }
        }
    }

    Ok(Graph { nodes, edges })
}

pub fn write_cache(kb_root: &Path, graph: &Graph) -> Result<()> {
    let path = kb_root
        .join(".biorouter-knowledge")
        .join("graph-cache.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(graph)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn read_cache(kb_root: &Path) -> Result<Option<Graph>> {
    let path = kb_root
        .join(".biorouter-knowledge")
        .join("graph-cache.json");
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&s)?))
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

fn page_basename(logical: &str) -> &str {
    logical
        .rsplit('/')
        .next()
        .unwrap_or(logical)
        .trim_end_matches(".md")
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

    #[test]
    fn get_graph_self_heals_stale_cache_with_scaffold_nodes() {
        use crate::knowledge::types::{Graph, GraphNode, PageKind};
        let (_d, kb) = build_sample();
        // Simulate an old cache that still contains the scaffold `index` hub.
        let stale = Graph {
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
        write_cache(&kb, &stale).unwrap();

        let svc = KnowledgeService::new(_d.path().to_path_buf());
        let g = svc.get_graph("k").unwrap();
        assert!(
            g.nodes.iter().all(|n| n.id != "index"),
            "stale scaffold cache must be re-derived, got {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        // And the cache on disk is now healed.
        let healed = read_cache(&kb).unwrap().unwrap();
        assert!(healed.nodes.iter().all(|n| n.id != "index"));
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
            "---\ntitle: Wanjun Gu — Google Scholar (URL reference)\nkind: source\n---\n\
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
