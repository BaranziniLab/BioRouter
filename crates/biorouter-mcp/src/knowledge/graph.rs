use crate::knowledge::{
    raw,
    store::{self, PageRef},
    types::{Graph, GraphEdge, GraphNode, PageKind},
};
use anyhow::Result;
use std::path::Path;

const KNOWLEDGE_LINK_RE: &str = r"\[\[([^\]]+)\]\]";

pub fn derive(kb_root: &Path) -> Result<Graph> {
    let pages = store::list_pages(kb_root, None)?;
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
            path: p.path.clone(),
        });
    }

    // Source nodes inherit credibility from raw/<id>/meta.yaml.
    for src in raw::list_sources(kb_root)? {
        let logical = format!("knowledge/sources/{}.md", src.id);
        if let Some(node_id) = id_for_path.get(&logical) {
            if let Some(n) = nodes.iter_mut().find(|n| &n.id == node_id) {
                n.credibility_tier = Some(src.credibility.tier);
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
            let target_label = cap.get(1).unwrap().as_str().trim();
            let key = slug(&target_label.to_lowercase());
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
    fn cache_write_then_read() {
        let (_d, kb) = build_sample();
        let g = derive(&kb).unwrap();
        write_cache(&kb, &g).unwrap();
        let back = read_cache(&kb).unwrap().unwrap();
        assert_eq!(back, g);
    }
}
