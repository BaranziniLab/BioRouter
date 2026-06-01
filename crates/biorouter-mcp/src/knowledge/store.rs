use crate::knowledge::{git::GitRepo, types::ChangeKind};
use anyhow::{Context, Result};
use bm25::{Language, SearchEngineBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRef {
    pub path: String,
    pub title: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContent {
    pub path: String,
    pub content: String,
    pub frontmatter: serde_yaml::Value,
}

pub fn list_pages(kb_root: &Path, prefix: Option<&str>) -> Result<Vec<PageRef>> {
    let knowledge_dir = kb_root.join("knowledge");
    if !knowledge_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk_md(&knowledge_dir, &knowledge_dir, prefix, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn walk_md(base: &Path, dir: &Path, prefix: Option<&str>, out: &mut Vec<PageRef>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_md(base, &p, prefix, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            let logical = format!("knowledge/{rel}");
            if let Some(pre) = prefix {
                if !logical.starts_with(pre) {
                    continue;
                }
            }
            let body = std::fs::read_to_string(&p)?;
            let (fm, _) = split_frontmatter(&body);
            let title = fm
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| p.file_stem().unwrap().to_str().unwrap())
                .to_string();
            let kind = fm
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("note")
                .to_string();
            out.push(PageRef {
                path: logical,
                title,
                kind,
            });
        }
    }
    Ok(())
}

pub fn read_page(kb_root: &Path, path: &str) -> Result<PageContent> {
    let abs = resolve_page_path(kb_root, path)?;
    let raw =
        std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
    let (fm, body) = split_frontmatter(&raw);
    Ok(PageContent {
        path: path.to_string(),
        content: body,
        frontmatter: fm,
    })
}

pub fn write_page(
    kb_root: &Path,
    path: &str,
    content: &str,
    commit_message: &str,
    txn_branch: Option<&str>,
) -> Result<Option<String>> {
    let abs = resolve_page_path(kb_root, path)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = abs.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(tmp, &abs)?;

    let repo = GitRepo::open(kb_root)?;
    if let Some(_branch) = txn_branch {
        // Caller has already switched HEAD to the txn branch via begin_txn.
        let sha = repo.commit_on_txn_in_progress(commit_message)?;
        Ok(Some(sha))
    } else {
        let sha = repo.commit_all(ChangeKind::Manual, commit_message, None)?;
        Ok(Some(sha))
    }
}

fn resolve_page_path(kb_root: &Path, logical: &str) -> Result<std::path::PathBuf> {
    if !logical.starts_with("knowledge/")
        && logical != "index.md"
        && logical != "schema.md"
        && logical != "log.md"
    {
        anyhow::bail!("page path must start with knowledge/ or be index.md/schema.md/log.md");
    }
    if logical.contains("..") {
        anyhow::bail!("path traversal not allowed");
    }
    Ok(kb_root.join(logical))
}

pub fn split_frontmatter(s: &str) -> (serde_yaml::Value, String) {
    if let Some(rest) = s.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            if let (Some(fm), Some(body)) = (rest.get(..end), rest.get(end + 5..)) {
                if let Ok(v) = serde_yaml::from_str(fm) {
                    return (v, body.to_string());
                }
            }
        }
    }
    (serde_yaml::Value::Null, s.to_string())
}

// ── BM25 search ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

pub fn search(kb_root: &Path, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let mut docs: Vec<(String, String)> = Vec::new(); // (logical_path, body)
    let knowledge_dir = kb_root.join("knowledge");
    if knowledge_dir.exists() {
        collect_docs_under(&knowledge_dir, &knowledge_dir, "knowledge", &mut docs)?;
    }
    let raw_dir = kb_root.join("raw");
    if raw_dir.exists() {
        for entry in std::fs::read_dir(&raw_dir)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            let source_md = entry.path().join("source.md");
            if source_md.exists() {
                let body = std::fs::read_to_string(&source_md)?;
                docs.push((format!("raw/{id}/source.md"), body));
            }
        }
    }
    if docs.is_empty() {
        return Ok(Vec::new());
    }

    // Build a corpus with u32 indices; SearchEngineBuilder::with_corpus auto-generates IDs.
    let bodies: Vec<String> = docs.iter().map(|(_, b)| b.clone()).collect();
    let engine = SearchEngineBuilder::<u32>::with_corpus(Language::English, bodies).build();
    let results = engine.search(query, limit);
    let hits: Vec<SearchHit> = results
        .into_iter()
        .map(|sr| {
            let idx = sr.document.id as usize;
            let (path, body) = &docs[idx];
            SearchHit {
                path: path.clone(),
                score: sr.score,
                snippet: snippet_of(body, query, 200),
            }
        })
        .collect();
    Ok(hits)
}

fn collect_docs_under(
    base: &Path,
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, String)>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_docs_under(base, &p, prefix, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            let logical = format!("{prefix}/{rel}");
            let body = std::fs::read_to_string(&p)?;
            out.push((logical, body));
        }
    }
    Ok(())
}

fn snippet_of(body: &str, query: &str, max_len: usize) -> String {
    let needle = query.to_ascii_lowercase();
    let hay = body.to_ascii_lowercase();
    if let Some(pos) = hay.find(&needle) {
        let start = pos.saturating_sub(60);
        let end = (pos + needle.len() + 140).min(body.len());
        let mut snippet = body[start..end].replace('\n', " ");
        if snippet.len() > max_len {
            snippet.truncate(max_len);
        }
        snippet
    } else {
        body.chars()
            .take(max_len)
            .collect::<String>()
            .replace('\n', " ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb_root = dir.path().join("k");
        (dir, kb_root)
    }

    #[test]
    fn write_then_read_roundtrip() {
        let (_dir, kb) = fresh();
        let body = "---\ntitle: HRV\nkind: entity\n---\n\nBody text.";
        write_page(&kb, "knowledge/entities/hrv.md", body, "add HRV", None).unwrap();
        let p = read_page(&kb, "knowledge/entities/hrv.md").unwrap();
        assert_eq!(p.frontmatter["title"], serde_yaml::Value::from("HRV"));
        assert_eq!(p.content.trim(), "Body text.");
    }

    #[test]
    fn list_pages_sorted_and_filtered() {
        let (_dir, kb) = fresh();
        write_page(&kb, "knowledge/entities/b.md", "---\ntitle: B\n---\n", "b", None).unwrap();
        write_page(&kb, "knowledge/concepts/a.md", "---\ntitle: A\n---\n", "a", None).unwrap();
        let all = list_pages(&kb, None).unwrap();
        let paths: Vec<_> = all.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["knowledge/concepts/a.md", "knowledge/entities/b.md"]);
        let only_entities = list_pages(&kb, Some("knowledge/entities/")).unwrap();
        assert_eq!(only_entities.len(), 1);
    }

    #[test]
    fn rejects_path_traversal() {
        let (_dir, kb) = fresh();
        let err = write_page(&kb, "knowledge/../escape.md", "x", "x", None).unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn rejects_paths_outside_knowledge() {
        let (_dir, kb) = fresh();
        let err = write_page(&kb, "raw/x.md", "x", "x", None).unwrap_err();
        assert!(err.to_string().contains("knowledge/"));
    }

    #[test]
    fn search_returns_relevant_hits() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntitle: HRV\nkind: entity\n---\n\nHeart rate variability is a key marker.",
            "a",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/concepts/sleep.md",
            "---\ntitle: Sleep\nkind: concept\n---\n\nSleep quality affects HRV directly.",
            "b",
            None,
        )
        .unwrap();
        let hits = search(&kb, "heart rate variability", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.path.ends_with("hrv.md")));
    }

    #[test]
    fn search_returns_empty_when_no_match() {
        let (_dir, kb) = fresh();
        write_page(
            &kb,
            "knowledge/entities/x.md",
            "---\ntitle: X\n---\nbody",
            "a",
            None,
        )
        .unwrap();
        let hits = search(&kb, "zzznonexistent", 5).unwrap();
        assert!(hits.is_empty());
    }
}
