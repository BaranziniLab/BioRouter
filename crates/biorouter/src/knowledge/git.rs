use crate::knowledge::types::{ChangeKind, HistoryEntry};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

pub struct GitRepo {
    inner: git2::Repository,
}

impl GitRepo {
    pub fn init(path: &Path) -> Result<Self> {
        let inner = git2::Repository::init(path)
            .with_context(|| format!("git init {}", path.display()))?;
        // Configure a deterministic identity so tests are reproducible.
        let mut cfg = inner.config()?;
        cfg.set_str("user.name", "BioRouter Knowledge")?;
        cfg.set_str("user.email", "knowledge@biorouter.local")?;
        cfg.set_str("commit.gpgsign", "false")?;
        Ok(Self { inner })
    }

    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self { inner: git2::Repository::open(path)? })
    }

    pub fn commit_all(&self, kind: ChangeKind, summary: &str, delta: Option<&str>) -> Result<String> {
        let mut index = self.inner.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map(|c| vec![c]).unwrap_or_default();
        let msg = render_message(kind, summary, delta);
        let oid = self.inner.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &msg,
            &tree,
            &parents,
        )?;
        Ok(oid.to_string())
    }

    pub fn log(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut walk = self.inner.revwalk()?;
        walk.push_head()?;
        walk.set_sorting(git2::Sort::TIME)?;
        let mut out = Vec::new();
        for oid in walk.flatten().take(limit) {
            let commit = self.inner.find_commit(oid)?;
            let parsed = parse_message(commit.message().unwrap_or(""));
            out.push(HistoryEntry {
                commit_sha: oid.to_string(),
                kind: parsed.kind,
                summary: parsed.summary,
                delta: parsed.delta,
                timestamp: DateTime::<Utc>::from_timestamp(commit.time().seconds(), 0)
                    .unwrap_or_else(Utc::now),
            });
        }
        Ok(out)
    }
}

fn render_message(kind: ChangeKind, summary: &str, delta: Option<&str>) -> String {
    let kind_str = match kind {
        ChangeKind::Ingest => "ingest",
        ChangeKind::Link => "link",
        ChangeKind::Flag => "flag",
        ChangeKind::Query => "query",
        ChangeKind::Lint => "lint",
        ChangeKind::Restore => "restore",
        ChangeKind::Manual => "manual",
    };
    match delta {
        Some(d) => format!("[{kind_str}] {summary}\n\ndelta: {d}\n"),
        None => format!("[{kind_str}] {summary}\n"),
    }
}

struct Parsed {
    kind: ChangeKind,
    summary: String,
    delta: Option<String>,
}

fn parse_message(msg: &str) -> Parsed {
    let mut lines = msg.lines();
    let header = lines.next().unwrap_or("");
    let (kind, summary) = parse_header(header);
    let delta = msg.lines().find_map(|l| l.strip_prefix("delta: ").map(str::to_string));
    Parsed { kind, summary, delta }
}

fn parse_header(header: &str) -> (ChangeKind, String) {
    let kind = if let Some(rest) = header.strip_prefix('[') {
        if let Some((k, _)) = rest.split_once(']') {
            match k {
                "ingest" => ChangeKind::Ingest,
                "link" => ChangeKind::Link,
                "flag" => ChangeKind::Flag,
                "query" => ChangeKind::Query,
                "lint" => ChangeKind::Lint,
                "restore" => ChangeKind::Restore,
                _ => ChangeKind::Manual,
            }
        } else {
            ChangeKind::Manual
        }
    } else {
        ChangeKind::Manual
    };
    let summary = header
        .split_once(']')
        .map(|(_, s)| s.trim_start().to_string())
        .unwrap_or_else(|| header.to_string());
    (kind, summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_repo() {
        let dir = tempfile::tempdir().unwrap();
        GitRepo::init(dir.path()).unwrap();
        assert!(dir.path().join(".git").exists());
    }

    #[test]
    fn commit_and_log_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "hello").unwrap();
        let sha = repo
            .commit_all(ChangeKind::Ingest, "first source", Some("+1 page"))
            .unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].commit_sha, sha);
        assert_eq!(log[0].kind, ChangeKind::Ingest);
        assert_eq!(log[0].summary, "first source");
        assert_eq!(log[0].delta.as_deref(), Some("+1 page"));
    }

    #[test]
    fn multiple_commits_ordered_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "1").unwrap();
        repo.commit_all(ChangeKind::Ingest, "one", None).unwrap();
        std::fs::write(dir.path().join("b.md"), "2").unwrap();
        repo.commit_all(ChangeKind::Lint, "two", None).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log[0].summary, "two");
        assert_eq!(log[1].summary, "one");
    }
}
