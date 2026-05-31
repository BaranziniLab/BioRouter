use crate::knowledge::types::{ChangeKind, HistoryEntry};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

pub struct GitRepo {
    inner: git2::Repository,
}

impl GitRepo {
    pub fn init(path: &Path) -> Result<Self> {
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        let inner = git2::Repository::init_opts(path, &opts)
            .with_context(|| format!("git init {}", path.display()))?;
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

pub struct Txn {
    pub branch: String,
}

impl GitRepo {
    pub fn begin_txn(&self, label: &str) -> Result<Txn> {
        let id = uuid::Uuid::new_v4();
        let branch = format!("txn/{label}-{id}", label = slugify(label));
        let head = self.inner.head()?.peel_to_commit()?;
        self.inner.branch(&branch, &head, false)?;
        self.inner.set_head(&format!("refs/heads/{branch}"))?;
        Ok(Txn { branch })
    }

    pub fn commit_on_txn(&self, _txn: &Txn, message: &str) -> Result<String> {
        // Same as commit_all but caller already on the txn branch.
        let mut index = self.inner.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head()?.peel_to_commit()?;
        let oid = self.inner.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
        Ok(oid.to_string())
    }

    pub fn commit_txn(&self, txn: &Txn, kind: ChangeKind, summary: &str, delta: Option<&str>) -> Result<String> {
        // Squash-merge txn branch onto main as one commit.
        let main = self.inner.find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_name = main.name()?.unwrap_or("main").to_string();
        let txn_commit = self.inner.find_branch(&txn.branch, git2::BranchType::Local)?
            .get().peel_to_commit()?;
        let txn_tree = txn_commit.tree()?;
        let main_commit = main.get().peel_to_commit()?;

        let sig = self.inner.signature()?;
        let msg = render_message(kind, summary, delta);
        let new_oid = self.inner.commit(
            Some(&format!("refs/heads/{main_name}")),
            &sig, &sig, &msg, &txn_tree, &[&main_commit],
        )?;

        // Move HEAD back to main and check out the new tree.
        self.inner.set_head(&format!("refs/heads/{main_name}"))?;
        self.inner.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        // Delete txn branch.
        self.inner.find_branch(&txn.branch, git2::BranchType::Local)?.delete()?;
        Ok(new_oid.to_string())
    }

    pub fn abort_txn(&self, txn: &Txn) -> Result<()> {
        let main = self.inner.find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_name = main.name()?.unwrap_or("main").to_string();
        self.inner.set_head(&format!("refs/heads/{main_name}"))?;
        self.inner.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        self.inner.find_branch(&txn.branch, git2::BranchType::Local)?
            .delete()?;
        Ok(())
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

impl GitRepo {
    pub fn read_file_at(&self, sha: &str, path: &str) -> Result<Option<String>> {
        let oid = git2::Oid::from_str(sha)?;
        let commit = self.inner.find_commit(oid)?;
        let tree = commit.tree()?;
        let entry = match tree.get_path(Path::new(path)) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };
        let obj = entry.to_object(&self.inner)?;
        let blob = obj.as_blob().ok_or_else(|| anyhow::anyhow!("not a blob"))?;
        Ok(Some(String::from_utf8_lossy(blob.content()).to_string()))
    }

    pub fn restore_to(&self, sha: &str, summary: &str) -> Result<String> {
        let oid = git2::Oid::from_str(sha)?;
        let target = self.inner.find_commit(oid)?;
        let target_tree = target.tree()?;
        let head = self.inner.head()?.peel_to_commit()?;
        let sig = self.inner.signature()?;
        let msg = render_message(ChangeKind::Restore, summary, Some(&format!("→ {}", &sha[..7])));
        let new_oid = self.inner.commit(
            Some("HEAD"), &sig, &sig, &msg, &target_tree, &[&head],
        )?;
        // Check out the new commit so working tree matches.
        self.inner.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        Ok(new_oid.to_string())
    }
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

    #[test]
    fn txn_lifecycle_squash_merges_into_main() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("seed.md"), "seed").unwrap();
        repo.commit_all(ChangeKind::Manual, "seed", None).unwrap();

        let txn = repo.begin_txn("ingest paper X").unwrap();
        std::fs::write(dir.path().join("p1.md"), "1").unwrap();
        repo.commit_on_txn(&txn, "step 1").unwrap();
        std::fs::write(dir.path().join("p2.md"), "2").unwrap();
        repo.commit_on_txn(&txn, "step 2").unwrap();
        let final_sha = repo
            .commit_txn(&txn, ChangeKind::Ingest, "Paper X", Some("+2 pages"))
            .unwrap();

        let log = repo.log(10).unwrap();
        assert_eq!(log[0].commit_sha, final_sha);
        assert_eq!(log[0].summary, "Paper X");
        assert_eq!(log[0].kind, ChangeKind::Ingest);
        assert_eq!(log.len(), 2, "seed + squashed-ingest only — no intermediate commits");
    }

    #[test]
    fn txn_abort_leaves_main_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("seed.md"), "seed").unwrap();
        repo.commit_all(ChangeKind::Manual, "seed", None).unwrap();
        let pre = repo.log(10).unwrap();

        let txn = repo.begin_txn("doomed").unwrap();
        std::fs::write(dir.path().join("doom.md"), "x").unwrap();
        repo.commit_on_txn(&txn, "bad").unwrap();
        repo.abort_txn(&txn).unwrap();

        let post = repo.log(10).unwrap();
        assert_eq!(pre, post);
        assert!(!dir.path().join("doom.md").exists(), "working tree restored");
    }

    #[test]
    fn preview_state_returns_file_at_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "v1").unwrap();
        let sha1 = repo.commit_all(ChangeKind::Manual, "v1", None).unwrap();
        std::fs::write(dir.path().join("a.md"), "v2").unwrap();
        repo.commit_all(ChangeKind::Manual, "v2", None).unwrap();
        let v1 = repo.read_file_at(&sha1, "a.md").unwrap();
        assert_eq!(v1.as_deref(), Some("v1"));
    }

    #[test]
    fn restore_state_creates_new_commit_with_old_tree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.md"), "v1").unwrap();
        let sha1 = repo.commit_all(ChangeKind::Manual, "v1", None).unwrap();
        std::fs::write(dir.path().join("a.md"), "v2").unwrap();
        repo.commit_all(ChangeKind::Manual, "v2", None).unwrap();
        let new_sha = repo.restore_to(&sha1, "restore to v1").unwrap();
        // Working tree should now contain v1.
        let body = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
        assert_eq!(body, "v1");
        // History still grows forward.
        let log = repo.log(10).unwrap();
        assert_eq!(log[0].commit_sha, new_sha);
        assert_eq!(log[0].kind, ChangeKind::Restore);
        assert_eq!(log.len(), 3);
    }
}
