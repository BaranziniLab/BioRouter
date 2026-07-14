//! `ShadowRepo` — a shadow git repository whose object DB lives in the app data
//! dir while its work-tree is the session's `working_dir`.
//!
//! The load-bearing safety property: we never create a `.git` in the working
//! dir and never touch the user's `.git`/index/refs. `GIT_DIR` points into the
//! data dir and the work-tree is bound at runtime via `set_workdir(.., false)`
//! (the `false` = do NOT write a gitlink into the work-tree). This mirrors the
//! Knowledge-base `GitRepo` (`biorouter-mcp/src/knowledge/git.rs`) but with a
//! detached GIT_DIR + external work-tree instead of a `.git` under the tree.

use anyhow::{Context, Result};
use ignore::gitignore::Gitignore;
use ignore::WalkBuilder;
use std::path::{Component, Path, PathBuf};

/// Our private checkpoint ref — never `HEAD`, never a branch the user sees.
const CHECKPOINT_REF: &str = "refs/biorouter/checkpoints";

/// Size/space guards applied while walking the work-tree.
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    /// Files larger than this are skipped (never committed as blobs), matching
    /// OpenCode's >2 MiB exclusion. A file over the cap is therefore never a
    /// "created-since" candidate on restore, so a huge pre-existing file is not
    /// deleted by a rewind.
    pub max_file_bytes: u64,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
        }
    }
}

pub struct ShadowRepo {
    inner: git2::Repository,
    worktree: PathBuf,
}

impl ShadowRepo {
    /// Open an existing shadow repo at `git_dir`, or initialise one, binding its
    /// work-tree to `worktree` without writing anything into `worktree`.
    pub fn open_or_init(git_dir: &Path, worktree: &Path) -> Result<Self> {
        let inner = match git2::Repository::open_bare(git_dir) {
            Ok(repo) => repo,
            Err(_) => {
                std::fs::create_dir_all(git_dir)
                    .with_context(|| format!("create shadow git dir {}", git_dir.display()))?;
                let mut opts = git2::RepositoryInitOptions::new();
                // Bare: objects live directly under git_dir, no work-tree is
                // implied, and crucially git2 never writes a gitlink file. We
                // attach the real work-tree at runtime below.
                opts.bare(true);
                opts.initial_head("main");
                let repo = git2::Repository::init_opts(git_dir, &opts)
                    .with_context(|| format!("git init shadow repo {}", git_dir.display()))?;
                {
                    let mut cfg = repo.config()?;
                    cfg.set_str("user.name", "BioRouter Checkpoints")?;
                    cfg.set_str("user.email", "checkpoints@biorouter.local")?;
                    cfg.set_str("commit.gpgsign", "false")?;
                    // Allow checkout into the runtime work-tree (a bare repo
                    // otherwise refuses to write files out).
                    cfg.set_bool("core.bare", false)?;
                }
                repo
            }
        };
        // `false` = do NOT create a `.git` gitlink under the work-tree. This is
        // what keeps the user's project directory untouched.
        inner
            .set_workdir(worktree, false)
            .context("bind shadow work-tree")?;
        Ok(Self {
            inner,
            worktree: worktree.to_path_buf(),
        })
    }

    /// Commit the current work-tree into the shadow repo on `CHECKPOINT_REF`.
    /// Returns `Some((commit_sha, tree_sha))`, or `None` when the eligible
    /// work-tree exceeds `max_tree_bytes` (skip — too big to snapshot cheaply).
    /// The `tree_sha` is the O(1) dedup key — an identical work-tree yields the
    /// same tree oid.
    pub fn snapshot(
        &self,
        ignore: &Gitignore,
        caps: &Caps,
        max_tree_bytes: Option<u64>,
    ) -> Result<Option<(String, String)>> {
        let paths = self.eligible_paths(ignore, caps)?;

        if let Some(cap) = max_tree_bytes {
            let mut total = 0u64;
            for rel in &paths {
                if let Ok(m) = std::fs::metadata(self.worktree.join(rel)) {
                    total = total.saturating_add(m.len());
                    if total > cap {
                        return Ok(None);
                    }
                }
            }
        }

        let mut index = self.inner.index()?;
        // Build the index from scratch each time so a deletion in the work-tree
        // is reflected (add-only would leave stale entries).
        index.clear()?;
        for rel in &paths {
            // add_path reads the file from the bound work-tree and hashes it.
            index
                .add_path(rel)
                .with_context(|| format!("index add {}", rel.display()))?;
        }
        index.write()?;
        let tree_oid = index.write_tree()?;
        let tree_sha = tree_oid.to_string();
        let tree = self.inner.find_tree(tree_oid)?;

        let sig = self.signature()?;
        let parent = self
            .inner
            .find_reference(CHECKPOINT_REF)
            .ok()
            .and_then(|r| r.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map(|c| vec![c]).unwrap_or_default();
        let oid = self.inner.commit(
            Some(CHECKPOINT_REF),
            &sig,
            &sig,
            "checkpoint",
            &tree,
            &parents,
        )?;
        Ok(Some((oid.to_string(), tree_sha)))
    }

    /// Restore the work-tree to `commit_sha`'s tree: overwrite tracked files and
    /// delete snapshot-eligible files created since (leaving ignored/oversized
    /// files, which were never snapshotted, alone). Returns the paths written.
    pub fn restore_files(
        &self,
        commit_sha: &str,
        ignore: &Gitignore,
        caps: &Caps,
    ) -> Result<Vec<PathBuf>> {
        let oid = git2::Oid::from_str(commit_sha)?;
        let commit = self.inner.find_commit(oid)?;
        let tree = commit.tree()?;

        // Paths present in the target checkpoint.
        let mut target_paths = std::collections::HashSet::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                let name = entry.name().unwrap_or("");
                target_paths.insert(format!("{dir}{name}"));
            }
            git2::TreeWalkResult::Ok
        })?;

        // Snapshot-eligible files that exist now but not in the target were
        // created (or grew into eligibility) since — remove them.
        let current = self.eligible_paths(ignore, caps)?;
        for rel in &current {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if !target_paths.contains(rel_str.as_str()) {
                let abs = self.worktree.join(rel);
                let _ = std::fs::remove_file(&abs);
            }
        }

        // Force-materialise the target tree into the work-tree.
        let mut co = git2::build::CheckoutBuilder::new();
        co.force();
        self.inner
            .checkout_tree(tree.as_object(), Some(&mut co))
            .context("checkout checkpoint tree")?;

        // Re-point our ref + index at the restored tree so a subsequent snapshot
        // parents onto it and diffs correctly.
        self.inner.reference(CHECKPOINT_REF, oid, true, "restore")?;
        let mut index = self.inner.index()?;
        index.read_tree(&tree)?;
        index.write()?;

        let mut restored: Vec<PathBuf> = target_paths.iter().map(PathBuf::from).collect();
        restored.sort();
        Ok(restored)
    }

    /// Read a file's contents at a checkpoint commit (`None` if absent).
    pub fn read_file_at(&self, commit_sha: &str, path: &str) -> Result<Option<String>> {
        let oid = git2::Oid::from_str(commit_sha)?;
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

    /// Paths (relative to the work-tree) that a snapshot would include: honours
    /// `.gitignore` (via `WalkBuilder`), the caller's extra `ignore` matcher
    /// (`.biorouterignore` + config globs), and the per-file size cap; always
    /// skips `.git`.
    fn eligible_paths(&self, ignore: &Gitignore, caps: &Caps) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        let mut walker = WalkBuilder::new(&self.worktree);
        walker
            // Include dotfiles (we skip `.git` explicitly below).
            .hidden(false)
            .parents(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            // Honour .gitignore even when the work-tree is not itself a git repo.
            .require_git(false);
        for entry in walker.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if path == self.worktree {
                continue;
            }
            let rel = match path.strip_prefix(&self.worktree) {
                Ok(r) => r,
                Err(_) => continue,
            };
            // Never descend into / capture the user's own git metadata.
            if rel
                .components()
                .any(|c| matches!(c, Component::Normal(n) if n == ".git"))
            {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(true);
            if is_dir {
                continue;
            }
            if ignore.matched(path, false).is_ignore() {
                continue;
            }
            match entry.metadata() {
                Ok(m) if m.len() > caps.max_file_bytes => continue,
                Ok(_) => {}
                Err(_) => continue,
            }
            out.push(rel.to_path_buf());
        }
        out.sort();
        Ok(out)
    }

    fn signature(&self) -> Result<git2::Signature<'static>> {
        self.inner
            .signature()
            .or_else(|_| {
                git2::Signature::now("BioRouter Checkpoints", "checkpoints@biorouter.local")
            })
            .map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignore::gitignore::GitignoreBuilder;

    fn empty_ignore(root: &Path) -> Gitignore {
        GitignoreBuilder::new(root).build().unwrap()
    }

    fn open(git_dir: &Path, worktree: &Path) -> ShadowRepo {
        ShadowRepo::open_or_init(git_dir, worktree).unwrap()
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let git_dir = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let repo = open(git_dir.path(), work.path());
        let ign = empty_ignore(work.path());
        let caps = Caps::default();

        std::fs::write(work.path().join("a.txt"), "v1").unwrap();
        std::fs::write(work.path().join("keep.txt"), "keep").unwrap();
        let (sha1, _tree1) = repo.snapshot(&ign, &caps, None).unwrap().unwrap();

        // Modify a.txt, delete keep.txt, add new.txt.
        std::fs::write(work.path().join("a.txt"), "v2").unwrap();
        std::fs::remove_file(work.path().join("keep.txt")).unwrap();
        std::fs::write(work.path().join("new.txt"), "new").unwrap();

        let restored = repo.restore_files(&sha1, &ign, &caps).unwrap();
        assert!(restored.iter().any(|p| p.ends_with("a.txt")));

        // a.txt restored to v1
        assert_eq!(
            std::fs::read_to_string(work.path().join("a.txt")).unwrap(),
            "v1"
        );
        // keep.txt re-created
        assert_eq!(
            std::fs::read_to_string(work.path().join("keep.txt")).unwrap(),
            "keep"
        );
        // new.txt (created since) removed
        assert!(!work.path().join("new.txt").exists());
    }

    #[test]
    fn identical_worktree_same_tree_sha() {
        let git_dir = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let repo = open(git_dir.path(), work.path());
        let ign = empty_ignore(work.path());
        let caps = Caps::default();

        std::fs::write(work.path().join("a.txt"), "same").unwrap();
        let (_c1, t1) = repo.snapshot(&ign, &caps, None).unwrap().unwrap();
        let (_c2, t2) = repo.snapshot(&ign, &caps, None).unwrap().unwrap();
        assert_eq!(
            t1, t2,
            "unchanged work-tree must dedup to the same tree sha"
        );
    }

    #[test]
    fn honors_gitignore_and_size_cap() {
        let git_dir = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let repo = open(git_dir.path(), work.path());
        let caps = Caps { max_file_bytes: 8 };

        std::fs::write(work.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(work.path().join("ignored.txt"), "should skip").unwrap();
        std::fs::write(work.path().join("big.bin"), vec![b'x'; 64]).unwrap();
        std::fs::write(work.path().join("small.txt"), "ok").unwrap();
        let ign = empty_ignore(work.path());

        let (sha, _t) = repo.snapshot(&ign, &caps, None).unwrap().unwrap();
        assert!(repo.read_file_at(&sha, "small.txt").unwrap().is_some());
        assert!(
            repo.read_file_at(&sha, "ignored.txt").unwrap().is_none(),
            ".gitignore'd file must not be committed"
        );
        assert!(
            repo.read_file_at(&sha, "big.bin").unwrap().is_none(),
            "over-cap file must not be committed"
        );
    }

    #[test]
    fn non_git_worktree_snapshots_fine() {
        let git_dir = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let repo = open(git_dir.path(), work.path());
        let ign = empty_ignore(work.path());
        std::fs::write(work.path().join("f.txt"), "hi").unwrap();
        let (sha, _t) = repo
            .snapshot(&ign, &Caps::default(), None)
            .unwrap()
            .unwrap();
        assert_eq!(
            repo.read_file_at(&sha, "f.txt").unwrap().as_deref(),
            Some("hi")
        );
    }

    #[test]
    fn never_writes_gitlink_into_worktree() {
        let git_dir = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let repo = open(git_dir.path(), work.path());
        let ign = empty_ignore(work.path());
        std::fs::write(work.path().join("f.txt"), "hi").unwrap();
        repo.snapshot(&ign, &Caps::default(), None)
            .unwrap()
            .unwrap();
        assert!(
            !work.path().join(".git").exists(),
            "shadow repo must never create a .git in the work-tree"
        );
    }

    #[test]
    fn user_git_repo_left_untouched() {
        // The load-bearing isolation invariant: snapshotting a work-tree that is
        // itself a user git repo must leave the user's HEAD/index/refs unchanged.
        let git_dir = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();

        let user_repo = git2::Repository::init(work.path()).unwrap();
        {
            let mut cfg = user_repo.config().unwrap();
            cfg.set_str("user.name", "User").unwrap();
            cfg.set_str("user.email", "user@example.com").unwrap();
        }
        std::fs::write(work.path().join("tracked.txt"), "u1").unwrap();
        let mut idx = user_repo.index().unwrap();
        idx.add_path(Path::new("tracked.txt")).unwrap();
        idx.write().unwrap();
        let tree_oid = idx.write_tree().unwrap();
        let tree = user_repo.find_tree(tree_oid).unwrap();
        let sig = user_repo.signature().unwrap();
        let head_before = user_repo
            .commit(Some("HEAD"), &sig, &sig, "user commit", &tree, &[])
            .unwrap()
            .to_string();

        let repo = open(git_dir.path(), work.path());
        let ign = empty_ignore(work.path());
        std::fs::write(work.path().join("agent-made.txt"), "x").unwrap();
        repo.snapshot(&ign, &Caps::default(), None)
            .unwrap()
            .unwrap();

        // User's HEAD is byte-for-byte the same commit.
        let head_after = user_repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        assert_eq!(head_before, head_after, "user HEAD must not move");
        // User's index still has exactly the one tracked file.
        let user_idx = user_repo.index().unwrap();
        assert_eq!(user_idx.len(), 1, "user index must be untouched");
    }
}
