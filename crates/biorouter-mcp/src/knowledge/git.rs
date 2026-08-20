use crate::knowledge::types::{ChangeKind, HistoryEntry};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

/// One spelling of the lock's relative path, shared with `service::kb_lock_path`
/// (which takes it) and `brkb::walk` (which keeps it out of an archive for the
/// same reason this keeps it out of a commit).
use crate::knowledge::paths::KB_WRITE_LOCK_REL as WRITE_LOCK_PATH;

/// The one directory that holds curated knowledge. `raw/`, `log.md`, `index.md`
/// and `schema.md` are all bookkeeping around it.
const KNOWLEDGE_DIR: &str = "knowledge";

/// The oid of a commit's `knowledge/` subtree, or `None` when the commit has no
/// such directory (git does not track empty ones, so a brand-new KB has none).
fn knowledge_tree_id(commit: &git2::Commit) -> Result<Option<git2::Oid>> {
    let tree = commit.tree()?;
    Ok(tree.get_name(KNOWLEDGE_DIR).map(|entry| entry.id()))
}

fn stage_all(index: &mut git2::Index) -> Result<()> {
    let write_lock = Path::new(WRITE_LOCK_PATH);
    if index.get_path(write_lock, 0).is_some() {
        index.remove_path(write_lock)?;
    }

    let mut skip_write_lock = |path: &Path, _matched_pathspec: &[u8]| {
        if path == write_lock {
            1
        } else {
            0
        }
    };
    index.add_all(
        ["*"].iter(),
        git2::IndexAddOption::DEFAULT,
        Some(&mut skip_write_lock),
    )?;
    Ok(())
}

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
        cfg.set_str("user.name", "Biorouter Knowledge")?;
        cfg.set_str("user.email", "knowledge@biorouter.local")?;
        cfg.set_str("commit.gpgsign", "false")?;
        Ok(Self { inner })
    }

    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            inner: git2::Repository::open(path)?,
        })
    }

    pub fn commit_all(
        &self,
        kind: ChangeKind,
        summary: &str,
        delta: Option<&str>,
    ) -> Result<String> {
        let mut index = self.inner.index()?;
        stage_all(&mut index)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map(|c| vec![c]).unwrap_or_default();
        let msg = render_message(kind, summary, delta);
        let oid = self
            .inner
            .commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parents)?;
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
    let delta = msg
        .lines()
        .find_map(|l| l.strip_prefix("delta: ").map(str::to_string));
    Parsed {
        kind,
        summary,
        delta,
    }
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
        stage_all(&mut index)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head()?.peel_to_commit()?;
        let oid = self
            .inner
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
        Ok(oid.to_string())
    }

    /// Did this transaction write any *knowledge*?
    ///
    /// `commit_txn` squash-commits the txn branch's *tree* onto main, and git is
    /// happy to record a commit whose tree is byte-identical to its parent's. So
    /// a sub-agent that wrote nothing still produced a commit sha, and every
    /// caller downstream read that sha as proof the work happened (issue #71).
    ///
    /// Comparing the *whole* tree does not answer the question, though: three of
    /// the sub-agent's tools write outside `knowledge/` — `kb_append_log`
    /// appends to `log.md`, `kb_add_raw_source` materialises under `raw/`, and
    /// `kb_write_page` accepts the top-level `index.md` — and any one of them
    /// moves the tree on its own. Since `INGEST_PROCEDURE` asks for all of them,
    /// a provider that died after the log line would leave a run that announced
    /// a digest it never performed, and a whole-tree check would wave it
    /// through. So the comparison is scoped to the `knowledge/` subtree, whose
    /// oid summarises every page beneath it: differing oids mean at least one
    /// knowledge page was added, changed or removed, and nothing else does.
    pub fn txn_wrote_knowledge_pages(&self, txn: &Txn) -> Result<bool> {
        let main = self
            .inner
            .find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_knowledge =
            knowledge_tree_id(&main.get().peel_to_commit()?)?.map(|oid| oid.to_string());
        Ok(main_knowledge != self.txn_knowledge_tree_id(txn)?)
    }

    /// The `knowledge/` subtree oid at the tip of a transaction branch, as an
    /// opaque marker a caller can hold and compare later.
    ///
    /// [`Self::txn_wrote_knowledge_pages`] answers "did anything change since
    /// **main**", which is the right question only while main is the last thing
    /// that wrote knowledge. It stops being the right question the moment a
    /// caller seeds the transaction itself — BioOKF ingest materialises the
    /// source node before the sub-agent starts (DR-24) — because the seed alone
    /// then satisfies the check and issue #71's guarantee, that a commit sha
    /// means work happened, quietly stops holding. A caller that seeds takes
    /// this marker afterwards and compares against it instead.
    ///
    /// `Option<String>` and not `String`: a bundle with no `knowledge/`
    /// directory yet is a real state (an empty base), and "absent" has to
    /// compare unequal to "present and empty" rather than being papered over
    /// with a sentinel.
    pub fn txn_knowledge_tree_id(&self, txn: &Txn) -> Result<Option<String>> {
        Ok(knowledge_tree_id(
            &self
                .inner
                .find_branch(&txn.branch, git2::BranchType::Local)?
                .get()
                .peel_to_commit()?,
        )?
        .map(|oid| oid.to_string()))
    }

    pub fn commit_txn(
        &self,
        txn: &Txn,
        kind: ChangeKind,
        summary: &str,
        delta: Option<&str>,
    ) -> Result<String> {
        // Squash-merge txn branch onto main as one commit.
        let main = self
            .inner
            .find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_name = main.name()?.unwrap_or("main").to_string();
        let txn_commit = self
            .inner
            .find_branch(&txn.branch, git2::BranchType::Local)?
            .get()
            .peel_to_commit()?;
        let txn_tree = txn_commit.tree()?;
        let main_commit = main.get().peel_to_commit()?;

        let sig = self.inner.signature()?;
        let msg = render_message(kind, summary, delta);
        let new_oid = self.inner.commit(
            Some(&format!("refs/heads/{main_name}")),
            &sig,
            &sig,
            &msg,
            &txn_tree,
            &[&main_commit],
        )?;

        // Move HEAD back to main and check out the new tree.
        self.inner.set_head(&format!("refs/heads/{main_name}"))?;
        self.inner
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        // Delete txn branch.
        self.inner
            .find_branch(&txn.branch, git2::BranchType::Local)?
            .delete()?;
        Ok(new_oid.to_string())
    }

    pub fn abort_txn(&self, txn: &Txn) -> Result<()> {
        let main = self
            .inner
            .find_branch("main", git2::BranchType::Local)
            .or_else(|_| self.inner.find_branch("master", git2::BranchType::Local))?;
        let main_name = main.name()?.unwrap_or("main").to_string();
        self.inner.set_head(&format!("refs/heads/{main_name}"))?;
        self.inner
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        self.inner
            .find_branch(&txn.branch, git2::BranchType::Local)?
            .delete()?;
        self.repair_graph_cache_after_checkout();
        Ok(())
    }

    /// Re-derive `graph-cache.json` from whatever pages the checkout just left
    /// on disk.
    ///
    /// ## The defect
    ///
    /// The cache is a **tracked** file — the scaffolded `.gitignore` covers
    /// `raw/*/original.*`, `.crossref-cache/` and `write.lock`, and nothing
    /// else — and every rebuild in the subsystem runs *after* the commit that
    /// motivated it (`store::write_page` commits, then the service rebuilds).
    /// So the copy in `HEAD` is permanently one write behind the pages in
    /// `HEAD`. The force checkout above restores tracked files, faithfully, and
    /// that is precisely the problem: it installs a cache describing a base
    /// that is one page smaller than the one now on disk.
    ///
    /// Nothing downstream can recover from that, because a stale cache is a
    /// *valid* one. DR-13 has `read_cache` answer `Ok(None)` for a cache that is
    /// absent, unreadable, malformed or of an unknown version, and `get_graph`
    /// re-derives on that answer — but a well-formed cache from the previous
    /// commit passes every one of those checks and is served. The user's newest
    /// pages disappear from the Knowledge view and from `GET /graph` while
    /// sitting on disk, and the trigger is a *failed* operation, which is
    /// exactly when nobody goes looking for silent data loss.
    ///
    /// ## Why here and not at the twelve call sites
    ///
    /// `abort_txn` is called from ingest, query, lint, merge and `kb_abort_txn`,
    /// always as `let _ = repo.abort_txn(&txn)` on an error path. A repair
    /// bolted onto each of those is a repair the thirteenth caller will not
    /// have, and error paths are the ones least likely to be exercised. The
    /// checkout is what invalidates the cache, so the repair belongs beside the
    /// checkout.
    ///
    /// ## Why re-derive rather than preserve, or untrack
    ///
    /// Preserving the working copy across the checkout would be wrong whenever
    /// the transaction rebuilt the cache itself (ingest does): the preserved
    /// copy would then describe pages the abort just rolled back. Deriving from
    /// the pages that survived is the only answer that is right in both cases.
    ///
    /// Untracking the file — the other candidate fix — would stop git clobbering
    /// it, and `.brkb` export walks the working tree rather than the git history
    /// so a bundle would still carry it. But it only helps bases created
    /// *after* the change: `.gitignore` does not untrack what is already
    /// tracked, so every base on disk would keep the bug until something
    /// rewrote its index, and that something would be an automatic write to
    /// every knowledge base on the machine to fix a derived file. Not worth it.
    ///
    /// ## Failures are absorbed, and downgraded to "no cache"
    ///
    /// This returns nothing and the abort does not fail on it. The rollback the
    /// caller asked for has already happened and is the part that matters, and
    /// every caller discards the `Result` anyway. But a failed rebuild must not
    /// leave the stale file in place, or the silent-loss bug survives its own
    /// fix — so on any failure the file is removed, which is the one state DR-13
    /// guarantees `get_graph` repairs by re-deriving.
    fn repair_graph_cache_after_checkout(&self) {
        let Some(kb_root) = self.inner.workdir().map(Path::to_path_buf) else {
            // A bare repo has no pages to derive from. Not reachable for a
            // knowledge base, and not worth an error if it ever is.
            return;
        };
        let rebuilt = crate::knowledge::graph::derive(&kb_root)
            .and_then(|graph| crate::knowledge::graph::write_cache(&kb_root, &graph));
        if let Err(e) = rebuilt {
            let stale = crate::knowledge::graph::cache_path(&kb_root);
            tracing::warn!(
                "knowledge: could not rebuild the graph cache at {} after aborting a \
                 transaction, removing it so the next read re-derives: {e:#}",
                stale.display()
            );
            let _ = std::fs::remove_file(&stale);
        }
    }
}

fn slugify(s: &str) -> String {
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
        let msg = render_message(
            ChangeKind::Restore,
            summary,
            Some(&format!("→ {}", sha.get(..7).unwrap_or(sha))),
        );
        let new_oid = self
            .inner
            .commit(Some("HEAD"), &sig, &sig, &msg, &target_tree, &[&head])?;
        // Check out the new commit so working tree matches.
        self.inner
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
        Ok(new_oid.to_string())
    }
}

impl GitRepo {
    /// Commit on the currently-checked-out branch. Used by store::write_page
    /// when a txn is active and the caller has already switched HEAD.
    pub fn commit_on_txn_in_progress(&self, message: &str) -> Result<String> {
        let mut index = self.inner.index()?;
        stage_all(&mut index)?;
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree = self.inner.find_tree(tree_oid)?;
        let sig = self.inner.signature()?;
        let parent = self.inner.head()?.peel_to_commit()?;
        let oid = self
            .inner
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
        Ok(oid.to_string())
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
    fn commit_all_never_tracks_the_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).unwrap();
        let lock = dir.path().join(WRITE_LOCK_PATH);
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, "transient").unwrap();
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .unwrap();
        fs2::FileExt::lock_exclusive(&lock_file).unwrap();
        std::fs::write(dir.path().join("a.md"), "tracked").unwrap();

        let sha = repo
            .commit_all(ChangeKind::Manual, "skip lock", None)
            .unwrap();

        assert_eq!(repo.read_file_at(&sha, WRITE_LOCK_PATH).unwrap(), None);
        assert_eq!(
            repo.read_file_at(&sha, "a.md").unwrap().as_deref(),
            Some("tracked")
        );
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
        assert_eq!(
            log.len(),
            2,
            "seed + squashed-ingest only: no intermediate commits"
        );
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
        assert!(
            !dir.path().join("doom.md").exists(),
            "working tree restored"
        );
    }

    /// Aborting must not silently delete the newest pages **from the graph**.
    ///
    /// The bug this pins is entirely invisible from git's own point of view, and
    /// [`txn_abort_leaves_main_untouched`] above is green throughout it: the
    /// abort restores the tracked files exactly as it promises, and
    /// `graph-cache.json` is one of them. The trouble is what the committed copy
    /// of that file *says*. Every rebuild runs after the commit that motivated
    /// it, so the copy in `HEAD` is permanently one write behind the pages in
    /// `HEAD`, and a force checkout therefore installs a cache that is missing
    /// the last committed page. `read_cache` cannot save the reader — a stale
    /// cache is a perfectly valid one, so it is served rather than re-derived,
    /// and the page vanishes from the Knowledge view while sitting on disk.
    ///
    /// So the assertion is deliberately about the **cache's contents** and not
    /// about the file's existence or its mtime: every wrong implementation
    /// leaves a file there.
    ///
    /// The fixture goes through `KnowledgeService` rather than writing a repo by
    /// hand because the staleness is a property of the *order* production writes
    /// happen in — page, commit, then rebuild — and a hand-built cache would be
    /// whatever the test author decided to put in it.
    #[test]
    fn aborting_a_transaction_leaves_the_graph_cache_describing_the_pages_on_disk() {
        use crate::knowledge::{graph, service::KnowledgeService, store};

        let cached_paths = |kb: &Path| -> Vec<String> {
            let mut paths: Vec<String> = graph::read_cache(kb)
                .expect("reading the cache is not an error")
                .expect("a base that has been rebuilt has a cache")
                .nodes
                .iter()
                .map(|n| n.path.clone())
                .collect();
            paths.sort();
            paths
        };
        let page = |identifier: &str| {
            format!("---\ntype: Concept\nidentifier: {identifier}\n---\n\nbody\n")
        };

        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("kb", "kb", None).unwrap();
        let kb = svc.root().join("kb");

        // A committed page, with the rebuild after the commit — the order every
        // production write takes, and the whole reason the committed cache lags.
        store::write_page(&kb, "knowledge/concept/a.md", &page("A"), "add A", None).unwrap();
        svc.rebuild_graph_cache("kb").unwrap();
        assert!(
            cached_paths(&kb).contains(&"knowledge/concept/a.md".to_string()),
            "the fixture never got A into the cache"
        );

        // A transaction that writes a page and is then abandoned — a sub-agent
        // that ran out of steps, a provider that died, a merge that failed its
        // canonical check.
        let repo = GitRepo::open(&kb).unwrap();
        let txn = repo.begin_txn("doomed").unwrap();
        store::write_page(
            &kb,
            "knowledge/concept/b.md",
            &page("B"),
            "add B",
            Some(&txn.branch),
        )
        .unwrap();
        svc.rebuild_graph_cache("kb").unwrap();
        repo.abort_txn(&txn).unwrap();

        assert!(
            kb.join("knowledge/concept/a.md").exists(),
            "the abort deleted a committed page from disk"
        );
        assert!(
            !kb.join("knowledge/concept/b.md").exists(),
            "the abort left the transaction's page behind"
        );
        assert_eq!(
            cached_paths(&kb),
            vec!["knowledge/concept/a.md".to_string()],
            "the graph after the abort does not describe the pages on disk"
        );
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
