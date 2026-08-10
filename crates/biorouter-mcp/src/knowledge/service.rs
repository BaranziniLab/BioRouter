use crate::knowledge::{
    convert, credibility,
    git::GitRepo,
    manifest, paths, raw, registry,
    types::{Manifest, ModelRef, RegistryEntry, SourceMeta},
};
use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use fs2::FileExt as _;
use std::sync::Arc;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use tokio::sync::{Mutex, OwnedMutexGuard};

const DEFAULT_SCHEMA: &str = include_str!("schema_default.md");
const DEFAULT_INDEX: &str = "# Index\n\n_no pages yet_\n";
const DEFAULT_LOG: &str = "# Log\n\n";
const GITIGNORE: &str =
    "raw/*/original.*\n.biorouter-knowledge/.crossref-cache/\n.biorouter-knowledge/write.lock\n";

/// Cross-reference rules block appended to legacy `schema.md` files that
/// pre-date the Plan 5 Task 2 schema hardening. Kept in sync with the
/// equivalent block in `schema_default.md`. The unique substring
/// `"Cross-reference rules"` is used as the migration fingerprint.
const SCHEMA_CROSSREF_RULES: &str = r#"
### Cross-reference rules (the graph depends on these)

The knowledge graph is derived **purely** from `[[link]]` patterns in page
bodies. If you do not emit links, the graph will have nodes but no edges.

When you write or update any knowledge page:

1. Every mention of another entity or concept that has (or should have) its
   own page **must** be wrapped in `[[double brackets]]`. Match the target
   page's title exactly (case-insensitive); the deriver slugifies both sides.
   Good: `[[EPAS1]] interacts with [[HIF2A]] under [[hypoxia]].`
   Bad:  `EPAS1 interacts with HIF2A under hypoxia.`
2. Every source page **must** include a `## Related pages` section listing
   every entity/concept it touches, one `- [[Name]]` bullet per line.
3. Every entity/concept page **must** include a `## Sources` section with
   one `- [[source-id]]` bullet per supporting source.
4. Prefer linking over re-stating. If a fact lives on another page, write
   `See [[Page Name]]` instead of restating it.

The lint workflow (`kb_lint`) reports pages with no inbound links as orphans;
fix them by adding inbound `[[links]]` from related pages.
"#;

#[derive(Clone)]
pub struct KnowledgeService {
    root: PathBuf,
    locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

struct FileLockGuard {
    file: File,
}

impl FileLockGuard {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(Self { file })
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub struct KnowledgeWriteGuard {
    _process_guard: OwnedMutexGuard<()>,
    _file_guard: FileLockGuard,
}

/// One coherent knowledge-base selection for a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbSelection {
    /// The scope's knowledge bases, sorted. Every one is searchable, readable,
    /// and eligible to be the primary.
    pub kb_ids: Vec<String>,
    /// The hidden ids that produced `kb_ids`.
    pub hidden_kbs: Vec<String>,
    /// The write target for KB-less mutating calls. Always a member of
    /// `kb_ids`, or `None` when the scope has not chosen one.
    pub primary_kb: Option<String>,
}

/// What a caller wants to happen to the primary pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryUpdate<'a> {
    /// Leave the stored pointer alone. A set-only edit must never move it —
    /// this is what stops one surface's write clobbering another's choice.
    Unchanged,
    /// Forget the pointer *at this scope*. KB-less writes then fail until one
    /// is chosen — including in a session whose machine-wide default still
    /// names a base. See [`StoredPrimary::NoPrimary`].
    Clear,
    /// Drop this scope's own pointer so it falls back to the machine-wide one.
    /// The mirror of [`KnowledgeService::clear_hidden_for_session`], and
    /// distinct from [`PrimaryUpdate::Clear`]. At machine scope — where there
    /// is nothing above to inherit — the two coincide.
    Inherit,
    /// Pin this id. It must be a member of the *resulting* set.
    Set(&'a str),
}

/// The three states a scope's primary-pointer file can be in.
///
/// Two of them collapse into `None` under a naive `Option<String>` read, and
/// that collapse *was* a bug: a session that says "no primary here" is not a
/// session that has said nothing. Clearing a session's primary used to delete
/// its file, which is the encoding of "I have no opinion" — so the very next
/// read handed the session back the machine-wide default it had just rejected,
/// silently re-arming KB-less writes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StoredPrimary {
    /// No file. This scope has expressed nothing, so a session falls back to
    /// the machine-wide pointer.
    Inherit,
    /// A file holding one bare kb id.
    Pinned(String),
    /// A file that exists but is blank: an explicit "this scope has no
    /// primary", which does **not** inherit. Blank rather than a sentinel word
    /// so a lagging PATH-installed `biorouter` (see CLAUDE.md, "Runtime
    /// CLI-vs-app drift") reading `.active-kb` trims it to nothing and agrees.
    NoPrimary,
}

impl StoredPrimary {
    /// The pinned id, if any. Both `Inherit` and `NoPrimary` are "no id here";
    /// only [`KnowledgeService::primary_for_session`] cares which.
    fn pinned(&self) -> Option<&str> {
        match self {
            StoredPrimary::Pinned(id) => Some(id.as_str()),
            _ => None,
        }
    }
}

/// "No primary at this scope", spelled the way that scope can actually
/// represent it. A session needs the blank-file override so it does not
/// re-inherit; the machine has nothing above it, so removing the file is the
/// identical state and leaves no debris behind for users who never used the
/// feature.
fn no_primary_for(session_id: Option<&str>) -> StoredPrimary {
    match session_id {
        Some(_) => StoredPrimary::NoPrimary,
        None => StoredPrimary::Inherit,
    }
}

impl KnowledgeService {
    fn primary_session_path(&self, session_id: &str) -> PathBuf {
        let digest = raw::hash_bytes(session_id.as_bytes());
        paths::primary_kb_sessions_dir(self.root()).join(digest)
    }

    fn hidden_session_path(&self, session_id: &str) -> PathBuf {
        let digest = raw::hash_bytes(session_id.as_bytes());
        paths::hidden_kb_sessions_dir(self.root()).join(digest)
    }

    /// Read a primary-pointer file as the tri-state it actually is: absent ⇒
    /// `Inherit`, blank ⇒ `NoPrimary`, otherwise the bare kb id it holds. The
    /// on-disk format is one bare kb id — see [`paths::primary_kb_path`].
    fn read_primary_file_unlocked(&self, path: &Path) -> anyhow::Result<StoredPrimary> {
        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StoredPrimary::Inherit);
            }
            Err(err) => return Err(err.into()),
        };
        let trimmed = s.trim();
        if trimmed.is_empty() {
            Ok(StoredPrimary::NoPrimary)
        } else {
            Ok(StoredPrimary::Pinned(trimmed.to_string()))
        }
    }

    /// The pinned id, or `None` for either of the two "no id" states. Callers
    /// that must tell `Inherit` from `NoPrimary` read the tri-state directly.
    fn get_primary_path_unlocked(&self, path: &Path) -> anyhow::Result<Option<String>> {
        Ok(self
            .read_primary_file_unlocked(path)?
            .pinned()
            .map(ToOwned::to_owned))
    }

    /// Persist one of the three states. `NoPrimary` writes a *blank file* — it
    /// is a real, durable override, so it must leave something on disk that a
    /// later read can tell apart from "never chose".
    fn write_primary_file_unlocked(
        &self,
        path: &Path,
        value: &StoredPrimary,
    ) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match value {
            StoredPrimary::Pinned(id) => {
                crate::knowledge::paths::validate_kb_id(id)?;
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, id.as_bytes())?;
                std::fs::rename(tmp, path)?;
            }
            StoredPrimary::NoPrimary => {
                let tmp = path.with_extension("tmp");
                std::fs::write(&tmp, b"")?;
                std::fs::rename(tmp, path)?;
            }
            StoredPrimary::Inherit => {
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
        }

        Ok(())
    }

    fn get_hidden_path_unlocked(&self, path: &Path) -> anyhow::Result<Vec<String>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let s = std::fs::read_to_string(path)?;
        if s.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut hidden = serde_json::from_str::<Vec<String>>(&s)?;
        hidden.sort();
        hidden.dedup();
        Ok(hidden)
    }

    /// Normalise and validate a caller-supplied list of kb ids — trim, drop
    /// blanks, sort, dedupe, reject malformed ids. Kept separate from the write
    /// so a request can be rejected in full *before* anything touches disk.
    fn sanitize_kb_id_list(ids: &[String]) -> anyhow::Result<Vec<String>> {
        let mut sanitized = ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        sanitized.sort();
        sanitized.dedup();

        for id in &sanitized {
            crate::knowledge::paths::validate_kb_id(id)?;
        }

        Ok(sanitized)
    }

    /// Persist an already-sanitized hidden list. Infallible except for I/O, so
    /// it is safe to call once every decision in an operation has been made.
    fn write_hidden_file_unlocked(&self, path: &Path, ids: &[String]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // An empty list is written, not deleted: `get_hidden_for_session_or_persisted`
        // discriminates on file *existence*, so `[]` is how a session says
        // "I override, and I hide nothing". Deleting the file here made that
        // state unrepresentable and silently re-inherited the machine default.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(ids)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn set_hidden_path_unlocked(&self, path: &Path, ids: &[String]) -> anyhow::Result<()> {
        let sanitized = Self::sanitize_kb_id_list(ids)?;
        self.write_hidden_file_unlocked(path, &sanitized)
    }

    fn rewrite_hidden_path_refs_unlocked(
        &self,
        path: &Path,
        current_kb_id: &str,
        next_kb_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let hidden = self.get_hidden_path_unlocked(path)?;
        if hidden.is_empty() || !hidden.iter().any(|id| id == current_kb_id) {
            return Ok(());
        }

        let mut next_hidden = hidden
            .into_iter()
            .filter(|id| id != current_kb_id)
            .collect::<Vec<_>>();
        if let Some(next_kb_id) = next_kb_id {
            next_hidden.push(next_kb_id.to_string());
        }
        self.set_hidden_path_unlocked(path, &next_hidden)
    }

    /// Follow a rename, or absorb a delete, in every session's primary file.
    ///
    /// `next_kb_id = None` means the base is gone. That must leave the session
    /// with an explicit **no primary** (a blank file), not with no file at all:
    /// deleting the file is the encoding of "this session never chose", so a
    /// session that had deliberately pinned the deleted base would come back
    /// pointed at the machine-wide default it never asked for.
    fn rewrite_session_primary_refs_unlocked(
        &self,
        current_kb_id: &str,
        next_kb_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let dir = paths::primary_kb_sessions_dir(self.root());
        if !dir.exists() {
            return Ok(());
        }

        let next = match next_kb_id {
            Some(id) => StoredPrimary::Pinned(id.to_string()),
            None => StoredPrimary::NoPrimary,
        };

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file() {
                continue;
            }
            if !is_session_digest(&entry.file_name()) {
                continue;
            }

            if self.read_primary_file_unlocked(&path)?.pinned() == Some(current_kb_id) {
                self.write_primary_file_unlocked(&path, &next)?;
            }
        }

        Ok(())
    }

    fn rewrite_hidden_refs_unlocked(
        &self,
        current_kb_id: &str,
        next_kb_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let global_path = paths::hidden_kbs_path(self.root());
        self.rewrite_hidden_path_refs_unlocked(&global_path, current_kb_id, next_kb_id)?;

        let dir = paths::hidden_kb_sessions_dir(self.root());
        if !dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() || !is_session_digest(&entry.file_name()) {
                continue;
            }
            self.rewrite_hidden_path_refs_unlocked(&entry.path(), current_kb_id, next_kb_id)?;
        }

        Ok(())
    }

    fn slugify_kb_name(name: &str) -> String {
        let mut slug = String::with_capacity(name.len());
        let mut last_was_dash = false;

        for ch in name.chars().flat_map(|c| c.to_lowercase()) {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                slug.push(ch);
                last_was_dash = false;
            } else if !slug.is_empty() && !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }

        while slug.ends_with('-') {
            slug.pop();
        }

        slug
    }

    pub fn new(root: PathBuf) -> Self {
        let svc = Self {
            root,
            locks: Arc::new(DashMap::new()),
        };
        // Issue #56. Best-effort: `new` is infallible and a failure here must
        // not stop the app from opening. A root that never migrates reads every
        // base PUBLIC (the file is absent ⇒ "not migrated"), which is AR-2's
        // accepted direction, not a new one.
        if let Err(e) = svc.ensure_tiers_migrated() {
            tracing::warn!("knowledge: could not migrate kb tiers: {e:#}");
        }
        svc
    }

    pub fn new_default() -> Result<Self> {
        Ok(Self::new(paths::knowledge_root()?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn root_lock_path(&self) -> PathBuf {
        self.root.join(".knowledge-root.lock")
    }

    fn kb_lock_path(&self, kb_id: &str) -> PathBuf {
        paths::kb_root(&self.root, kb_id).join(paths::KB_WRITE_LOCK_REL)
    }

    fn lock_root(&self) -> Result<FileLockGuard> {
        FileLockGuard::acquire(&self.root_lock_path())
    }

    /// Take the root lock and raise `kb_id` on **both** of issue #56's axes: to
    /// the caller's tier, and to the set of institutions whose content it holds
    /// (DR-26 / Task 50 Step 1).
    ///
    /// For callers OUTSIDE this module. Inside it — `create_base`,
    /// `import_brkb`, `delete_base` — the lock is already held, so those call
    /// `tier::*_unlocked` directly (through [`Self::stamp_new_base_unlocked`],
    /// which is this function's unlocked twin). Calling this from there
    /// deadlocks.
    ///
    /// ⚠ **One method rather than two that callers are asked to pair.** It was
    /// two, and review found the consequences: the five production call sites
    /// paired them by convention, in two different orders (so a failure between
    /// them left a *public* base carrying an owner at one site and a claimed
    /// base at public tier at another), each taking and releasing the root lock
    /// separately. And a caller that raised only the tier would put an
    /// institution's content into a base no institution is recorded as owning —
    /// which reads as unclaimed and is therefore reachable from every other
    /// institution's model. Neither is expressible now: there is one call, one
    /// lock, one order.
    pub fn raise_tier_and_affiliation(
        &self,
        kb_id: &str,
        caller_is_private: bool,
        caller: &crate::knowledge::affiliation::CallerAffiliation,
    ) -> Result<()> {
        let _lock = self.lock_root()?;
        crate::knowledge::tier::raise_unlocked(&self.root, kb_id, caller_is_private)?;
        crate::knowledge::tier::raise_affiliation_unlocked(&self.root, kb_id, caller)
    }

    /// Stamp a brand-new base on **both** of issue #56's axes, inside a root
    /// lock the caller already holds.
    ///
    /// ⚠ **The pairing is this function, not a convention.** `create_base_as`
    /// and `import_brkb` are the two tools whose subject id is minted by the
    /// call, so neither can go through the `call_tool` seam that pairs the two
    /// raises for the other seventeen — and neither is visible to
    /// `server::tests::every_tool_that_ratchets_the_tier_also_records_the_callers_institution`,
    /// which is parameterised over a base that already exists. Task 50 shipped
    /// with both of them raising the tier alone: the affiliation axis was
    /// laundered by `kb_export` + `kb_import`, both endpoints Private, no gate
    /// crossed. One function that does both is what stops that from being
    /// re-introduced by someone reading only one of the call sites.
    fn stamp_new_base_unlocked(
        &self,
        kb_id: &str,
        caller_is_private: bool,
        owners: std::collections::BTreeSet<String>,
    ) -> Result<()> {
        crate::knowledge::tier::raise_unlocked(&self.root, kb_id, caller_is_private)?;
        crate::knowledge::tier::add_owners_unlocked(&self.root, kb_id, owners)
    }

    /// Take the root lock and SET `kb_id`'s tier on the user's behalf (issue #56
    /// DR-18) — the one call in the tree that can lower one.
    ///
    /// It sits beside [`Self::raise_tier`] and shares its lock discipline and its
    /// deadlock rule: never call it from `create_base` / `import_brkb` /
    /// `delete_base`, which are already inside `lock_root()`.
    ///
    /// The `&UserKbTierChange` is a proof-of-user with a private field. This
    /// wrapper can accept one and cannot make one — the only construction site
    /// in the tree is the HTTP handler behind the user-action header, pinned by
    /// `tier_user::tests::the_proof_of_user_is_constructed_in_exactly_one_place`.
    pub fn set_tier_by_user(
        &self,
        kb_id: &str,
        tier: crate::knowledge::types::KbTier,
        ok: &crate::knowledge::tier_user::UserKbTierChange,
    ) -> Result<()> {
        let _lock = self.lock_root()?;
        crate::knowledge::tier_user::set_unlocked(&self.root, kb_id, tier, ok)
    }

    /// The mirror of [`Self::raise_tier`], for a base that has gone away.
    pub fn forget_tier(&self, kb_id: &str) -> Result<()> {
        let _lock = self.lock_root()?;
        crate::knowledge::tier::forget_unlocked(&self.root, kb_id)
    }

    /// Idempotent, and cheap on the common path: it stats `.kb-tiers` BEFORE
    /// taking the lock and returns immediately when it exists, so the ~90
    /// `KnowledgeService::new` calls in the test suite do not each `flock`.
    fn ensure_tiers_migrated(&self) -> Result<()> {
        if crate::knowledge::paths::kb_tiers_path(&self.root).exists() {
            return Ok(());
        }
        if !self.root.exists() {
            return Ok(()); // no bases yet; the first create_base registers
        }
        let _lock = self.lock_root()?;
        crate::knowledge::tier::ensure_migrated_unlocked(&self.root)
    }

    /// Acquire an exclusive lock for `kb_id`. Held until the returned guard is dropped.
    /// Used by macros to serialize concurrent writers against the same KB.
    pub async fn lock_kb(&self, kb_id: &str) -> Result<KnowledgeWriteGuard> {
        let m = self
            .locks
            .entry(kb_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let process_guard = m.lock_owned().await;
        let file_guard = FileLockGuard::acquire(&self.kb_lock_path(kb_id))?;
        Ok(KnowledgeWriteGuard {
            _process_guard: process_guard,
            _file_guard: file_guard,
        })
    }

    /// Create a base on behalf of the user (no model involved), so it is born
    /// PUBLIC and unclaimed. Model-facing callers use [`Self::create_base_as`]
    /// instead.
    pub fn create_base(&self, id: &str, name: &str, color: Option<&str>) -> Result<Manifest> {
        self.create_base_as(
            id,
            name,
            color,
            false,
            &crate::knowledge::affiliation::CallerAffiliation::Unstated,
        )
    }

    /// Create a base and stamp it with the creating session's tier, both inside
    /// **one** root-lock transaction (issue #56).
    ///
    /// `create_base` + a separate `raise_tier` would be two transactions with a
    /// window between them in which a lock-free `is_private` reader sees a
    /// private session's brand-new base as PUBLIC, and in which a failing raise
    /// returns `Err` to the caller while a PUBLIC base persists on disk. That is
    /// the same reasoning that keeps `import_brkb`'s stamp inside its own single
    /// store write, applied to the other tool whose subject id does not exist
    /// before the call.
    ///
    /// `caller_affiliation` is stamped in the same transaction and for the same
    /// reason (issue #56 DR-26 / Task 50): the tier is decided **at creation**,
    /// not at the first ingest (DR-18(b)), and a base born in a UCSF chat that
    /// recorded no owner would read as unclaimed — reachable from every other
    /// institution's model — until something happened to write into it.
    pub fn create_base_as(
        &self,
        id: &str,
        name: &str,
        color: Option<&str>,
        caller_is_private: bool,
        caller_affiliation: &crate::knowledge::affiliation::CallerAffiliation,
    ) -> Result<Manifest> {
        let _lock = self.lock_root()?;
        paths::validate_kb_id(id)?;
        let kb_root = paths::kb_root(&self.root, id);
        if kb_root.exists() {
            anyhow::bail!("kb '{id}' already exists at {}", kb_root.display());
        }
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("entities"))?;
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("concepts"))?;
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("sources"))?;
        std::fs::create_dir_all(paths::kb_knowledge_dir(&self.root, id).join("notes"))?;
        std::fs::create_dir_all(paths::kb_raw_dir(&self.root, id))?;
        std::fs::create_dir_all(paths::kb_internal_dir(&self.root, id))?;

        let m = Manifest {
            id: id.to_string(),
            name: name.to_string(),
            color: color.unwrap_or("#5a6394").to_string(),
            created_at: Utc::now(),
            schema_version: 1,
            default_model: None,
        };
        manifest::save(&kb_root, &m)?;

        std::fs::write(kb_root.join("schema.md"), DEFAULT_SCHEMA)?;
        std::fs::write(kb_root.join("index.md"), DEFAULT_INDEX)?;
        std::fs::write(kb_root.join("log.md"), DEFAULT_LOG)?;
        std::fs::write(kb_root.join(".gitignore"), GITIGNORE)?;

        let repo = GitRepo::init(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("create knowledge base {id}"),
            None,
        )
        .context("initial commit")?;

        registry::register(
            &self.root,
            RegistryEntry {
                id: id.to_string(),
                path: kb_root,
            },
        )?;
        self.rebuild_graph_cache(id)?;
        // Issue #56, decision (5a). A base with no entry reads PRIVATE
        // (unknown provenance), so an unregistered base would lock its own
        // creator out. `raise_unlocked` registers an absent id at the caller's
        // tier and can never lower an existing entry, so it subsumes
        // `register_public_if_absent_unlocked` for the `false` case that the
        // ~90 user-facing `create_base` call sites take. Inside the root lock,
        // so the `_unlocked` twin is the one that must be called — and in the
        // SAME transaction as the directory, so there is no window in which a
        // private session's new base reads PUBLIC.
        self.stamp_new_base_unlocked(
            id,
            caller_is_private,
            crate::knowledge::affiliation::contributed_owners(caller_affiliation),
        )?;
        Ok(m)
    }

    pub fn export_brkb(&self, kb_id: &str) -> Result<Vec<u8>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{kb_id}' not found");
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        // Issue #56, decision (2a): the archive carries a raise-only provenance
        // marker. No barrier here — this is the USER's download path too
        // (`GET /knowledge/bases/{id}/export`), and DR-14 governs what a MODEL
        // can reach. The model-facing location rule lives in `kb_export`.
        let is_private = crate::knowledge::tier::is_private(&self.root, kb_id);
        // Issue #56 DR-26 / Task 50: the archive carries the owners too, or an
        // export is a way to strip them. `Unknown` — an unreadable store — is
        // the one case with nothing honest to write: the archive grammar has no
        // "unknown ownership" value, and inventing one that fails OPEN is
        // exactly the inversion Step 0's migration warning names. Refuse
        // instead. The same machine cannot write any knowledge base either
        // (`load_for_write` bails on an unreadable store), so this is a broken
        // store surfacing at one more place rather than a new failure mode.
        let owners = match crate::knowledge::tier::affiliation(&self.root, kb_id).owners() {
            Some(owners) => owners.clone(),
            None => anyhow::bail!(
                "cannot export '{kb_id}': the knowledge-base classification store is unreadable, \
                 so whose content this base holds cannot be established and the archive would \
                 claim it belongs to nobody. Repair or remove {}",
                crate::knowledge::paths::kb_tiers_path(&self.root).display()
            ),
        };
        crate::knowledge::brkb::export(&kb_root, &mut buf, is_private, &owners)?;
        Ok(buf.into_inner())
    }

    /// `importer_is_private` is the tier of whoever asked for the import. The
    /// new base ends up at `max(archive marker, importer)` — the marker can only
    /// raise, never lower (issue #56, decision 2a).
    ///
    /// On DR-26's third axis the new base's owners are the **union** of what the
    /// archive carried and the importer's own institution, which is the same
    /// disjunction the tier takes one axis over:
    ///
    /// * The archive's owners, because the content is theirs and an export must
    ///   not be a way to strip a claim.
    /// * The importer's, because importing is an act by that institution's
    ///   session — the same reason a private importer privatises what it
    ///   imports.
    ///
    /// So a Stanford chat importing a UCSF archive lands a base owned by both,
    /// which `affiliation::reachable` puts out of reach of *both* their models.
    /// That is over-restrictive only in the case DR-26 exists to stop, and the
    /// innocent case (UCSF importing its own archive) is a no-op.
    pub fn import_brkb(
        &self,
        zip_bytes: &[u8],
        importer_is_private: bool,
        importer_affiliation: &crate::knowledge::affiliation::CallerAffiliation,
    ) -> Result<String> {
        let _lock = self.lock_root()?;
        std::fs::create_dir_all(&self.root)?;
        let cursor = std::io::Cursor::new(zip_bytes);
        let imported = crate::knowledge::brkb::import(cursor, &self.root)?;
        let new_id = imported.id;
        let provenance_private = imported.provenance_private;
        let mut owners = imported.owners;
        owners.extend(crate::knowledge::affiliation::contributed_owners(
            importer_affiliation,
        ));
        // Register in the top-level manifest.
        let path = paths::kb_root(&self.root, &new_id);
        crate::knowledge::registry::register(
            &self.root,
            crate::knowledge::types::RegistryEntry {
                id: new_id.clone(),
                path,
            },
        )?;
        // Issue #56. ONE store write, not a register followed by a raise.
        // `raise_unlocked` registers an absent id at the caller's tier
        // (`raise_is_monotone_and_registers_an_absent_base_at_the_callers_tier`),
        // so the pair was redundant — and it was worse than redundant: each call
        // ends in its own `save`, and `is_private` is deliberately lock-free, so
        // a reader landing between the two renames would have seen a base whose
        // private content was already fully extracted on disk classified PUBLIC.
        // With a single write the reader sees either the pre-import state (a
        // directory with no entry, which reads private) or the final tier.
        //
        // The floor is a disjunction, never the marker alone: a hostile archive
        // claiming "public" must not lower a private importer's base.
        self.stamp_new_base_unlocked(
            &new_id,
            provenance_private.unwrap_or(false) || importer_is_private,
            owners,
        )?;
        Ok(new_id)
    }

    pub fn list_bases(&self) -> Result<Vec<Manifest>> {
        let entries = registry::load(&self.root)?;
        let mut out = Vec::new();
        for e in entries {
            if let Ok(m) = manifest::load(&e.path) {
                out.push(m);
            }
        }
        Ok(out)
    }

    pub fn get_base(&self, id: &str) -> Result<Manifest> {
        paths::validate_kb_id(id)?;
        let kb_root = paths::kb_root(&self.root, id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{id}' not found");
        }
        manifest::load(&kb_root)
    }

    pub fn update_base(
        &self,
        id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<Manifest> {
        let _lock = self.lock_root()?;
        paths::validate_kb_id(id)?;
        let current_root = paths::kb_root(&self.root, id);
        if !current_root.exists() {
            anyhow::bail!("kb '{id}' not found");
        }

        let mut current = manifest::load(&current_root)?;
        let mut changed = false;
        let mut target_id = id.to_string();
        let mut target_root = current_root.clone();

        if let Some(name) = name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                anyhow::bail!("knowledge base name cannot be empty");
            }
            if current.name != trimmed {
                let next_id = Self::slugify_kb_name(trimmed);
                if next_id.is_empty() {
                    anyhow::bail!("knowledge base name must contain letters or numbers");
                }
                paths::validate_kb_id(&next_id)?;
                let next_root = paths::kb_root(&self.root, &next_id);
                if next_id != id && next_root.exists() {
                    anyhow::bail!("kb '{next_id}' already exists");
                }
                current.name = trimmed.to_string();
                current.id = next_id.clone();
                target_id = next_id;
                target_root = next_root;
                changed = true;
            }
        }

        if let Some(color) = color {
            let trimmed = color.trim();
            if trimmed.is_empty() {
                anyhow::bail!("knowledge base color cannot be empty");
            }
            if current.color != trimmed {
                current.color = trimmed.to_string();
                changed = true;
            }
        }

        if changed {
            let commit_message = if target_id != id {
                format!("rename knowledge base {id} to {}", current.id)
            } else {
                format!("update knowledge base {id} metadata")
            };

            if target_id != id {
                std::fs::rename(&current_root, &target_root)?;
                registry::replace(
                    &self.root,
                    id,
                    RegistryEntry {
                        id: target_id.clone(),
                        path: target_root.clone(),
                    },
                )?;

                if self.get_primary_persisted_unlocked()?.as_deref() == Some(id) {
                    self.set_primary_persisted_unlocked(Some(&target_id))?;
                }
                self.rewrite_session_primary_refs_unlocked(id, Some(&target_id))?;
                self.rewrite_hidden_refs_unlocked(id, Some(&target_id))?;
            }

            manifest::save(&target_root, &current)?;
            let repo = GitRepo::open(&target_root)?;
            repo.commit_all(
                crate::knowledge::types::ChangeKind::Manual,
                &commit_message,
                None,
            )?;
            self.rebuild_graph_cache(&current.id)?;
        }

        Ok(current)
    }

    pub fn set_default_model(&self, id: &str, model: Option<ModelRef>) -> Result<Manifest> {
        let _lock = self.lock_root()?;
        paths::validate_kb_id(id)?;
        let kb_root = paths::kb_root(&self.root, id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{id}' not found");
        }

        let mut current = manifest::load(&kb_root)?;
        if current.default_model == model {
            return Ok(current);
        }

        current.default_model = model;
        manifest::save(&kb_root, &current)?;
        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("set default knowledge model for {id}"),
            None,
        )?;
        Ok(current)
    }

    pub fn delete_base(&self, id: &str) -> Result<()> {
        let _lock = self.lock_root()?;
        paths::validate_kb_id(id)?;
        let kb_root = paths::kb_root(&self.root, id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{id}' not found");
        }

        registry::unregister(&self.root, id)?;
        if let Err(err) = std::fs::remove_dir_all(&kb_root) {
            let _ = registry::register(
                &self.root,
                RegistryEntry {
                    id: id.to_string(),
                    path: kb_root.clone(),
                },
            );
            return Err(err.into());
        }

        if self.get_primary_persisted_unlocked()?.as_deref() == Some(id) {
            self.set_primary_persisted_unlocked(None)?;
        }
        self.rewrite_session_primary_refs_unlocked(id, None)?;
        self.rewrite_hidden_refs_unlocked(id, None)?;
        // Issue #56: drop the tier with the base, so a later base reusing the
        // id is classified by its own creator. Inside the root lock.
        crate::knowledge::tier::forget_unlocked(&self.root, id)?;

        Ok(())
    }
}

impl KnowledgeService {
    fn find_existing_source_match(
        &self,
        kb_root: &Path,
        url: Option<&str>,
        sha256: &str,
    ) -> Result<(Option<SourceMeta>, Option<SourceMeta>)> {
        let mut by_url = None;
        let mut by_hash = None;

        for meta in raw::list_sources(kb_root)? {
            if by_url.is_none() && meta.url.as_deref() == url && url.is_some() {
                by_url = Some(meta.clone());
            }
            if by_hash.is_none() && meta.sha256 == sha256 {
                by_hash = Some(meta);
            }
            if by_url.is_some() && by_hash.is_some() {
                break;
            }
        }

        Ok((by_url, by_hash))
    }

    pub async fn add_raw_source(
        &self,
        kb_id: &str,
        input: convert::SourceInput,
        txn_branch: Option<&str>,
    ) -> Result<raw::RawWrite> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if !kb_root.exists() {
            anyhow::bail!("kb '{kb_id}' does not exist");
        }

        let converted = convert::convert(&input).await?;
        // Classify against the *converted* text, not just the raw input bytes,
        // so a paper's DOI / journal markers in the body are actually seen.
        let credibility =
            credibility::classify_with_text(&input, Some(&converted.markdown), None).await?;

        let title = humanize_source_title(&input, &converted);

        let (original_bytes, original_filename, url) = match &input {
            convert::SourceInput::File {
                bytes, filename, ..
            } => (Some(bytes.clone()), Some(filename.clone()), None),
            convert::SourceInput::Path(path) => (
                Some(std::fs::read(path)?),
                Some(
                    path.file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("source")
                        .to_string(),
                ),
                None,
            ),
            convert::SourceInput::Url(u) => (None, None, Some(u.clone())),
            convert::SourceInput::Text { .. } => (None, None, None),
        };

        let hash = match &original_bytes {
            Some(b) => raw::hash_bytes(b),
            None => raw::hash_bytes(converted.markdown.as_bytes()),
        };

        let (existing_by_url, existing_by_hash) =
            self.find_existing_source_match(&kb_root, url.as_deref(), &hash)?;

        if let Some(existing) = existing_by_hash.as_ref() {
            if existing_by_url
                .as_ref()
                .map(|meta| meta.id.as_str())
                .unwrap_or(existing.id.as_str())
                == existing.id
            {
                return Ok(raw::RawWrite {
                    source_id: existing.id.clone(),
                    source_md_path: format!("raw/{}/source.md", existing.id),
                    meta_path: format!("raw/{}/meta.yaml", existing.id),
                });
            }
        }

        let source_id = existing_by_url
            .as_ref()
            .map(|meta| meta.id.clone())
            .unwrap_or_else(|| raw::new_source_id(&title));

        let meta = SourceMeta {
            id: source_id.clone(),
            title,
            url,
            ingested_at: Utc::now(),
            sha256: hash,
            mime: converted.mime.clone(),
            original_filename,
            credibility,
        };

        let source_markdown = source_markdown_with_quality_banner(&converted);

        let written = raw::write_raw(
            &kb_root,
            original_bytes.as_deref(),
            meta.original_filename.clone().as_deref(),
            &source_markdown,
            meta,
        )?;

        let repo = GitRepo::open(&kb_root)?;
        let (summary, delta) = if existing_by_url.is_some() {
            (format!("refresh source {source_id}"), "~1 source")
        } else {
            (format!("ingested {source_id}"), "+1 source")
        };
        if let Some(_branch) = txn_branch {
            repo.commit_on_txn_in_progress(&summary)?;
        } else {
            repo.commit_all(
                crate::knowledge::types::ChangeKind::Ingest,
                &summary,
                Some(delta),
            )?;
        }
        self.rebuild_graph_cache(kb_id)?;
        Ok(written)
    }
}

/// Derive a human-readable title for a source, never a hash or UUID filename.
///
/// Order of preference:
///   1. The title extracted by the converter (PDF metadata title, HTML
///      `<title>`, explicit note title) — unless it itself looks machine
///      generated (e.g. `a64e171e-….pdf`).
///   2. The first usable heading / line of the converted markdown body.
///   3. The URL or filename as a last resort, cleaned of obvious noise.
fn humanize_source_title(input: &convert::SourceInput, converted: &convert::Converted) -> String {
    if let Some(t) = converted.title.as_ref() {
        let t = t.trim();
        if !t.is_empty() && !looks_machine_generated(t) {
            return t.to_string();
        }
    }

    // Explicit note titles always win when provided by the caller.
    if let convert::SourceInput::Text { title: Some(t), .. } = input {
        let t = t.trim();
        if !t.is_empty() && !looks_machine_generated(t) {
            return t.to_string();
        }
    }

    if let Some(t) = title_from_markdown(&converted.markdown) {
        return t;
    }

    // Fall back to the source locator, cleaned up.
    let fallback = match input {
        convert::SourceInput::Url(u) => u.clone(),
        convert::SourceInput::File { filename, .. } => filename.clone(),
        convert::SourceInput::Path(path) => path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("source")
            .to_string(),
        convert::SourceInput::Text { .. } => String::new(),
    };
    if !fallback.is_empty() && !looks_machine_generated(&fallback) {
        return fallback;
    }
    let noun = match input {
        convert::SourceInput::Text { .. } => "note",
        _ => "source",
    };
    format!("Untitled {noun}")
}

/// Pull a plausible title out of converted markdown: the first markdown heading
/// (`# …`), or failing that the first reasonably-sized line of prose.
fn title_from_markdown(markdown: &str) -> Option<String> {
    // Skip a leading quality-warning blockquote if present.
    for line in markdown.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('>') {
            continue;
        }
        if let Some(h) = line.strip_prefix('#') {
            let h = h.trim_start_matches('#').trim();
            if h.len() >= 3 && !looks_machine_generated(h) {
                return Some(truncate_title(h));
            }
        }
    }
    // No heading — use the first substantial, sentence-like line.
    for line in markdown.lines() {
        let line = line.trim().trim_start_matches(['#', '>', '*', '-', ' ']);
        if line.len() >= 12
            && line.split_whitespace().count() >= 3
            && !looks_machine_generated(line)
        {
            return Some(truncate_title(line));
        }
    }
    None
}

fn truncate_title(s: &str) -> String {
    const MAX: usize = 160;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let truncated: String = s.chars().take(MAX).collect();
    match truncated.rsplit_once(' ') {
        Some((head, _)) => format!("{}…", head.trim_end()),
        None => format!("{truncated}…"),
    }
}

/// True when a string reads like a machine identifier (UUID, long hex hash, or
/// such with a file extension) rather than a human title. Mirrors the
/// frontend `looksMachineGenerated` guard so titles are clean at the source.
fn looks_machine_generated(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let stripped = t
        .rsplit_once('.')
        .map(|(stem, ext)| {
            if matches!(
                ext.to_ascii_lowercase().as_str(),
                "pdf" | "doc" | "docx" | "html" | "htm" | "txt" | "md" | "csv" | "pptx" | "ppt"
            ) {
                stem
            } else {
                t
            }
        })
        .unwrap_or(t);
    let compact: String = stripped
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' ' | '.'))
        .collect();
    if compact.len() >= 12 && compact.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    // UUID anywhere, with only noise (digits / "pdf" / separators) around it.
    if let Some(m) = find_uuid(t) {
        let remainder: String = t
            .replacen(m, " ", 1)
            .chars()
            .map(|c| if c.is_ascii_alphabetic() { c } else { ' ' })
            .collect();
        let words: Vec<&str> = remainder
            .split_whitespace()
            .filter(|w| {
                w.len() >= 3
                    && !matches!(
                        w.to_ascii_lowercase().as_str(),
                        "pdf" | "doc" | "docx" | "html"
                    )
            })
            .collect();
        if words.is_empty() {
            return true;
        }
    }
    false
}

/// Return the first UUID substring (8-4-4-4-12 hex) in `s`, if any.
fn find_uuid(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    let pattern = [8usize, 4, 4, 4, 12];
    let mut i = 0;
    while i + 36 <= bytes.len() {
        let mut ok = true;
        let mut pos = i;
        for (gi, &group) in pattern.iter().enumerate() {
            for _ in 0..group {
                if pos >= bytes.len() || !is_hex(bytes[pos]) {
                    ok = false;
                    break;
                }
                pos += 1;
            }
            if !ok {
                break;
            }
            if gi < pattern.len() - 1 {
                if pos >= bytes.len() || bytes[pos] != b'-' {
                    ok = false;
                    break;
                }
                pos += 1;
            }
        }
        if ok {
            // `i`/`pos` only ever land on ASCII hex / '-' bytes, so this is a
            // valid char boundary; `get` keeps clippy happy and is panic-free.
            return s.get(i..pos);
        }
        i += 1;
    }
    None
}

/// Surface poor extraction (scanned / image-based sources) instead of silently
/// storing an empty source.md: the banner is visible in the raw-source preview
/// and tells the digestion sub-agent the content is incomplete. Applied after
/// content hashing so dedup stays keyed on the real converted content.
fn source_markdown_with_quality_banner(converted: &convert::Converted) -> String {
    if converted.needs_llm_fallback {
        format!(
            "> **Warning: poor extraction quality.** This source appears to be \
             scanned or image-based; its text could not be extracted faithfully. \
             Treat the content below (if any) as incomplete.\n\n{}",
            converted.markdown
        )
    } else {
        converted.markdown.clone()
    }
}

/// Typed error returned by [`KnowledgeService::read_page`] so HTTP handlers can
/// map each variant onto the right status code (400 / 404 / 500) without
/// substring-matching the `Display` text.
#[derive(thiserror::Error, Debug)]
pub enum ReadPageError {
    #[error("invalid kb id: {0}")]
    InvalidKbId(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("page not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl KnowledgeService {
    /// Re-derive the knowledge graph from the on-disk pages and overwrite the
    /// cached `graph-cache.json`. Public so macros (and bug-fix migrations
    /// like the wiki-link deriver fix) can refresh stale caches without
    /// hand-crafting a commit.
    pub fn rebuild_graph_cache(&self, kb_id: &str) -> anyhow::Result<()> {
        let kb_root = paths::kb_root(&self.root, kb_id);
        let g = crate::knowledge::graph::derive(&kb_root)?;
        crate::knowledge::graph::write_cache(&kb_root, &g)?;
        Ok(())
    }

    /// One-shot, idempotent upgrade for KBs created before the schema gained
    /// explicit cross-reference rules (Plan 5 Task 2). If `schema.md` does
    /// not already mention `"Cross-reference rules"`, the rules block is
    /// appended in place and committed. User customisations elsewhere in
    /// the file are preserved.
    ///
    /// Returns `Ok(true)` if the schema was rewritten, `Ok(false)` if it was
    /// already up-to-date or the KB has no `schema.md`.
    pub fn migrate_schema_if_needed(&self, kb_id: &str) -> Result<bool> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let schema_path = kb_root.join("schema.md");
        if !schema_path.exists() {
            return Ok(false);
        }
        let current = std::fs::read_to_string(&schema_path).context("read schema.md")?;
        if current.contains("Cross-reference rules") {
            return Ok(false);
        }
        // Ensure a blank line separates whatever the user had from the new
        // section, even if their file did not end with a newline.
        let mut next = current;
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(SCHEMA_CROSSREF_RULES);
        std::fs::write(&schema_path, next).context("write schema.md")?;

        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            "migrate schema: add cross-reference rules",
            None,
        )
        .context("commit schema migration")?;
        Ok(true)
    }

    pub fn get_graph(&self, kb_id: &str) -> anyhow::Result<crate::knowledge::types::Graph> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        if let Some(g) = crate::knowledge::graph::read_cache(&kb_root)? {
            // Self-heal caches written by an older deriver: if the cache still
            // contains the scaffold `index`/`log` hub nodes (which the current
            // deriver excludes), re-derive once and rewrite the cache so existing
            // KBs pick up the fix without needing a fresh ingest.
            let has_scaffold = g.nodes.iter().any(|n| n.id == "index" || n.id == "log");
            if !has_scaffold {
                return Ok(g);
            }
            let fresh = crate::knowledge::graph::derive(&kb_root)?;
            let _ = crate::knowledge::graph::write_cache(&kb_root, &fresh);
            return Ok(fresh);
        }
        crate::knowledge::graph::derive(&kb_root)
    }

    /// Read the raw markdown body of a page (knowledge/*.md, raw/*/source.md,
    /// or top-level index.md/schema.md/log.md).
    ///
    /// Returns a typed [`ReadPageError`] so HTTP handlers can map each variant
    /// onto the right status code (400 / 404 / 500) without inspecting the
    /// `Display` text. Path traversal and out-of-scope paths are rejected.
    pub fn read_page(&self, kb_id: &str, rel_path: &str) -> Result<String, ReadPageError> {
        paths::validate_kb_id(kb_id).map_err(|e| ReadPageError::InvalidKbId(e.to_string()))?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let abs = crate::knowledge::store::resolve_readable_path(&kb_root, rel_path)
            .map_err(|e| ReadPageError::InvalidPath(e.to_string()))?;
        if !abs.exists() {
            return Err(ReadPageError::NotFound(rel_path.to_string()));
        }
        Ok(std::fs::read_to_string(&abs)?)
    }

    /// Read the persisted primary-KB id (set via the UI or `kb_set_active`).
    /// Returns `Ok(None)` if no file exists or the file is empty.
    pub fn get_primary_persisted(&self) -> anyhow::Result<Option<String>> {
        self.get_primary_persisted_unlocked()
    }

    fn get_primary_persisted_unlocked(&self) -> anyhow::Result<Option<String>> {
        self.get_primary_path_unlocked(&crate::knowledge::paths::primary_kb_path(self.root()))
    }

    /// Persist the primary-KB id. Pass `None` to clear.
    pub fn set_primary_persisted(&self, id: Option<&str>) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        self.set_primary_persisted_unlocked(id)
    }

    fn set_primary_persisted_unlocked(&self, id: Option<&str>) -> anyhow::Result<()> {
        let path = crate::knowledge::paths::primary_kb_path(self.root());
        let value = match id {
            Some(id) => StoredPrimary::Pinned(id.to_string()),
            // Machine scope: nothing above to inherit, so "no file" and "blank
            // file" are the same state. Prefer no file.
            None => no_primary_for(None),
        };
        self.write_primary_file_unlocked(&path, &value)
    }

    /// This session's *own* pinned id, ignoring the machine-wide default.
    /// `None` covers both "never chose" and "explicitly has none" — callers
    /// resolving the effective pointer must use
    /// [`Self::primary_for_session`], which knows the difference and applies
    /// the set-membership rule.
    pub fn get_primary_for_session(&self, session_id: &str) -> anyhow::Result<Option<String>> {
        self.get_primary_path_unlocked(&self.primary_session_path(session_id))
    }

    /// Pin (or explicitly un-pin) this session's primary. `kb_id = None` is an
    /// *override*: the session then has no primary even when the machine-wide
    /// default names a base. To go back to following that default, use
    /// [`Self::clear_primary_override_for_session`].
    pub fn set_primary_for_session(
        &self,
        session_id: &str,
        kb_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        let path = self.primary_session_path(session_id);
        let value = match kb_id {
            Some(id) => StoredPrimary::Pinned(id.to_string()),
            None => no_primary_for(Some(session_id)),
        };
        self.write_primary_file_unlocked(&path, &value)
    }

    /// Drop this session's primary override so it follows the machine-wide
    /// pointer again. The mirror of [`Self::clear_hidden_for_session`], and
    /// distinct from `set_primary_for_session(session_id, None)`, which is an
    /// override that pins nothing.
    pub fn clear_primary_override_for_session(&self, session_id: &str) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        let path = self.primary_session_path(session_id);
        self.write_primary_file_unlocked(&path, &StoredPrimary::Inherit)
    }

    pub fn get_hidden_persisted(&self) -> anyhow::Result<Vec<String>> {
        self.get_hidden_path_unlocked(&crate::knowledge::paths::hidden_kbs_path(self.root()))
    }

    /// Overwrite the machine-wide hidden list wholesale.
    ///
    /// Reach for [`Self::hide_kb`], [`Self::include_kb`] or
    /// [`Self::set_visible_kbs`] instead unless you already hold the complete
    /// list. This setter is atomic in itself, but the shape it invites —
    /// `get_hidden_persisted()`, edit, `set_hidden_persisted()` — is a
    /// read-modify-write across two unlocked calls, and two surfaces hiding two
    /// *different* bases at the same time will lose one of the two edits.
    pub fn set_hidden_persisted(&self, ids: &[String]) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        self.set_hidden_path_unlocked(&crate::knowledge::paths::hidden_kbs_path(self.root()), ids)?;
        self.repair_primary_unlocked(None)?;
        Ok(())
    }

    pub fn get_hidden_for_session(&self, session_id: &str) -> anyhow::Result<Vec<String>> {
        self.get_hidden_path_unlocked(&self.hidden_session_path(session_id))
    }

    pub fn get_hidden_for_session_or_persisted(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let path = self.hidden_session_path(session_id);
        if path.exists() {
            self.get_hidden_path_unlocked(&path)
        } else {
            self.get_hidden_persisted()
        }
    }

    /// Overwrite one session's hidden-list override wholesale. Same caveat as
    /// [`Self::set_hidden_persisted`]: prefer [`Self::hide_kb`],
    /// [`Self::include_kb`] or [`Self::set_visible_kbs`], which take the whole
    /// gesture under one lock and cannot lose a concurrent edit.
    pub fn set_hidden_for_session(&self, session_id: &str, ids: &[String]) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        self.set_hidden_path_unlocked(&self.hidden_session_path(session_id), ids)?;
        self.repair_primary_unlocked(Some(session_id))?;
        Ok(())
    }

    /// Drop a session's hidden-KB override so it inherits the machine-wide
    /// list again. Distinct from `set_hidden_for_session(sid, &[])`, which is
    /// an override that hides nothing.
    pub fn clear_hidden_for_session(&self, session_id: &str) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        let path = self.hidden_session_path(session_id);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.repair_primary_unlocked(Some(session_id))?;
        Ok(())
    }

    /// Remove the machine-wide hidden list entirely (equivalent to an empty
    /// list at this scope, but leaves no file behind).
    pub fn clear_hidden_persisted(&self) -> anyhow::Result<()> {
        let _lock = self.lock_root()?;
        let path = crate::knowledge::paths::hidden_kbs_path(self.root());
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.repair_primary_unlocked(None)?;
        Ok(())
    }

    /// The knowledge bases this scope may use, as ids, sorted.
    ///
    /// This is *the* set under the merged model: every base returned here is
    /// searchable by a `kb_id`-less `kb_search`, readable, and eligible to be
    /// the primary. `session_id = None` means "no session in scope" — the CLI
    /// and scheduled jobs — and falls back to the machine-wide hidden list.
    ///
    /// Sorted deliberately: the "lexicographically first member" promotion rule
    /// in [`Self::repair_primary_unlocked`] must not depend on registry
    /// insertion order, which differs between machines.
    pub fn session_kb_ids(&self, session_id: Option<&str>) -> anyhow::Result<Vec<String>> {
        let _lock = self.lock_root()?;
        self.session_kb_ids_unlocked(session_id)
    }

    fn session_kb_ids_unlocked(&self, session_id: Option<&str>) -> anyhow::Result<Vec<String>> {
        let hidden = self.hidden_for_scope_unlocked(session_id)?;
        Ok(self
            .installed_kb_ids_unlocked()?
            .into_iter()
            .filter(|id| !hidden.contains(id))
            .collect())
    }

    /// This scope's **primary** knowledge base: the write target for KB-less
    /// mutating calls and the default subject for single-base reads.
    ///
    /// Resolution is session file → machine file, and the result is returned
    /// only while it names a member of [`Self::session_kb_ids`]. A non-member
    /// yields `None` rather than promoting: promoting at read time would make
    /// "no primary" unreachable and let a KB-less *write* silently land in a
    /// base the user never ranked. Promotion happens once, at the moment the
    /// set changes, in [`Self::repair_primary_unlocked`].
    ///
    /// Only an *absent* session file inherits. A session file that exists but
    /// is blank is an explicit "no primary here" and stops the fallback dead —
    /// otherwise clearing a session's primary would be a no-op whenever the
    /// machine had one, and a KB-less write would silently re-arm.
    pub fn primary_for_session(&self, session_id: Option<&str>) -> anyhow::Result<Option<String>> {
        let _lock = self.lock_root()?;
        self.primary_for_session_unlocked(session_id)
    }

    fn primary_for_session_unlocked(
        &self,
        session_id: Option<&str>,
    ) -> anyhow::Result<Option<String>> {
        let stored = self.stored_primary_unlocked(session_id)?;
        let Some(stored) = stored.pinned() else {
            return Ok(None);
        };
        let ids = self.session_kb_ids_unlocked(session_id)?;
        Ok(ids.into_iter().find(|id| id == stored))
    }

    /// The tri-state that governs this scope, after the session → machine
    /// fallback but before the set-membership filter.
    fn stored_primary_unlocked(&self, session_id: Option<&str>) -> anyhow::Result<StoredPrimary> {
        let own = self.read_primary_file_unlocked(&self.primary_path_for_scope(session_id))?;
        self.effective_primary_unlocked(&own, session_id)
    }

    /// Resolve a scope's *own* tri-state into the one it is actually **using**:
    /// only an absent session file inherits, and only a session has anything
    /// above it to inherit from.
    ///
    /// Split out of [`Self::stored_primary_unlocked`] because the repair path
    /// needs both halves — it decides against the pointer the scope is using
    /// and writes the answer to the file the scope owns.
    fn effective_primary_unlocked(
        &self,
        own: &StoredPrimary,
        session_id: Option<&str>,
    ) -> anyhow::Result<StoredPrimary> {
        match (own, session_id) {
            (StoredPrimary::Inherit, Some(_)) => self
                .read_primary_file_unlocked(&crate::knowledge::paths::primary_kb_path(self.root())),
            _ => Ok(own.clone()),
        }
    }

    /// Every knowledge base installed on this machine, as ids, sorted — the
    /// universe the set is carved out of, and the answer to "does this id
    /// still exist?".
    fn installed_kb_ids_unlocked(&self) -> anyhow::Result<Vec<String>> {
        let mut ids = self
            .list_bases()?
            .into_iter()
            .map(|base| base.id)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    fn hidden_for_scope_unlocked(&self, session_id: Option<&str>) -> anyhow::Result<Vec<String>> {
        match session_id {
            Some(session_id) => self.get_hidden_for_session_or_persisted(session_id),
            None => self.get_hidden_persisted(),
        }
    }

    fn primary_path_for_scope(&self, session_id: Option<&str>) -> PathBuf {
        match session_id {
            Some(session_id) => self.primary_session_path(session_id),
            None => crate::knowledge::paths::primary_kb_path(self.root()),
        }
    }

    fn hidden_path_for_scope(&self, session_id: Option<&str>) -> PathBuf {
        match session_id {
            Some(session_id) => self.hidden_session_path(session_id),
            None => crate::knowledge::paths::hidden_kbs_path(self.root()),
        }
    }

    /// What a scope's primary file must become for "the primary is a member of
    /// the set" to hold against `next_ids`. `None` = leave the file alone.
    ///
    /// Pure, so the decision can be taken before anything is written.
    ///
    /// It reasons about `effective` — the pointer the scope is actually
    /// *using*, which for a session that has pinned nothing is the machine-wide
    /// one it inherits — and writes the answer to `own`, the file that scope
    /// owns. Reasoning about `own` alone made the common case wrong: most chats
    /// never pin their own primary, so hiding the base the chat displayed as
    /// *its* primary repaired nothing and the chat came back with no primary at
    /// all, while a chat that had pinned that very same base came back promoted.
    /// Same visible starting state, same click, two answers. Writing the
    /// promotion at session scope is what keeps the machine pointer intact for
    /// every other chat — a session that has moved off the default is exactly
    /// what a session-scope pin represents.
    ///
    /// It still never *invents* a pointer, which is a different thing from
    /// moving one the user already had:
    ///
    /// - the two "no id" states are returned untouched, so a scope that has
    ///   never chosen a primary is never handed one;
    /// - a pointer at a base that no longer **exists** is *cleared*, never
    ///   promoted — deletion clears, hiding promotes.
    ///
    /// That last distinction is the whole reason `installed` is a parameter.
    /// Promotion is only defensible for a base the user genuinely ranked and
    /// that is still there to rank — one that was merely hidden. A pointer at a
    /// base that has been deleted (or that an upgrade inherited from an older
    /// `.active-kb`) is a dangling reference: it correctly reads as no-primary,
    /// and treating it as "hidden" meant the next unrelated hide silently
    /// promoted a base nobody had chosen, which is precisely the invention the
    /// merged model forbids. A session that merely *inherits* a dangling
    /// pointer has nothing of its own to clear, so it is left alone: writing the
    /// blank override anyway would silently sever that chat from the machine
    /// default over a pointer it never chose, and both spellings read the same
    /// way today — no primary.
    fn repair_decision(
        own: &StoredPrimary,
        effective: &StoredPrimary,
        installed: &[String],
        next_ids: &[String],
        session_id: Option<&str>,
    ) -> Option<StoredPrimary> {
        let StoredPrimary::Pinned(stored) = effective else {
            return None;
        };
        if !installed.iter().any(|id| id == stored) {
            return match own {
                StoredPrimary::Inherit => None,
                _ => Some(no_primary_for(session_id)),
            };
        }
        if next_ids.iter().any(|id| id == stored) {
            return None;
        }
        Some(match next_ids.first() {
            Some(id) => StoredPrimary::Pinned(id.clone()),
            None => no_primary_for(session_id),
        })
    }

    /// Re-establish "the primary is a member of the set" for one scope after
    /// the set changed. Never invents a pointer where there was none. Callers
    /// must already hold the root lock.
    fn repair_primary_unlocked(&self, session_id: Option<&str>) -> anyhow::Result<Option<String>> {
        let path = self.primary_path_for_scope(session_id);
        let own = self.read_primary_file_unlocked(&path)?;
        let effective = self.effective_primary_unlocked(&own, session_id)?;
        let installed = self.installed_kb_ids_unlocked()?;
        let hidden = self.hidden_for_scope_unlocked(session_id)?;
        let next_ids = installed
            .iter()
            .filter(|id| !hidden.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(value) =
            Self::repair_decision(&own, &effective, &installed, &next_ids, session_id)
        {
            self.write_primary_file_unlocked(&path, &value)?;
            return Ok(value.pinned().map(ToOwned::to_owned));
        }
        Ok(effective.pinned().map(ToOwned::to_owned))
    }

    /// Read-only snapshot of a scope's selection.
    ///
    /// Taken under the root lock, because a `KbSelection` is a *claim*: its
    /// `primary_kb` is a member of its own `kb_ids`. Composing it from three
    /// separately-unlocked reads made that claim false whenever a writer landed
    /// between them — the reader could measure the set while a base was hidden
    /// and then read the pointer after it had been re-added and pinned, and
    /// hand back a primary that was not in the set it just reported.
    pub fn selection(&self, session_id: Option<&str>) -> anyhow::Result<KbSelection> {
        let _lock = self.lock_root()?;
        self.selection_unlocked(session_id)
    }

    /// One coherent snapshot from one pass over the store. Callers must already
    /// hold the root lock — the public [`Self::selection`] is the one that takes
    /// it, because acquiring `lock_root` twice in a call stack deadlocks.
    fn selection_unlocked(&self, session_id: Option<&str>) -> anyhow::Result<KbSelection> {
        let hidden_kbs = self.hidden_for_scope_unlocked(session_id)?;
        let kb_ids = self
            .installed_kb_ids_unlocked()?
            .into_iter()
            .filter(|id| !hidden_kbs.contains(id))
            .collect::<Vec<_>>();
        let primary_kb = self
            .stored_primary_unlocked(session_id)?
            .pinned()
            .filter(|id| kb_ids.iter().any(|known| known == id))
            .map(ToOwned::to_owned);

        Ok(KbSelection {
            kb_ids,
            hidden_kbs,
            primary_kb,
        })
    }

    /// Apply a set change and a primary change as one operation under one root
    /// lock, validating the primary against the **resulting** set. `hidden =
    /// None` leaves the set alone.
    ///
    /// Every helper called here is an `*_unlocked` variant: taking `lock_root`
    /// twice in one call stack deadlocks (the guard is an `flock` on one path,
    /// and a second acquisition from the same process blocks forever).
    pub fn set_selection(
        &self,
        session_id: Option<&str>,
        hidden: Option<&[String]>,
        primary: PrimaryUpdate<'_>,
    ) -> anyhow::Result<KbSelection> {
        let _lock = self.lock_root()?;
        self.apply_selection_unlocked(session_id, hidden, primary)
    }

    /// Drop one base from this scope's set, in one root-locked step.
    ///
    /// Prefer this over reading the hidden list, editing it, and writing it
    /// back: that pattern is a read-modify-write across two unlocked calls, so
    /// two surfaces hiding two *different* bases at the same time each write a
    /// list computed before the other's edit, and one of them silently
    /// disappears. Every operation on this API takes the whole gesture, so the
    /// racy shape is not expressible.
    ///
    /// Hiding a base that is not installed is accepted and idempotent — a base
    /// that no longer exists is already absent from the set, and a UI racing a
    /// concurrent delete should not have to care.
    pub fn hide_kb(
        &self,
        session_id: Option<&str>,
        kb_id: &str,
        primary: PrimaryUpdate<'_>,
    ) -> anyhow::Result<KbSelection> {
        let _lock = self.lock_root()?;
        crate::knowledge::paths::validate_kb_id(kb_id)?;
        let mut hidden = self.hidden_for_scope_unlocked(session_id)?;
        if !hidden.iter().any(|id| id == kb_id) {
            hidden.push(kb_id.to_string());
        }
        self.apply_selection_unlocked(session_id, Some(&hidden), primary)
    }

    /// Add one base to this scope's set (un-hide it), in one root-locked step.
    /// See [`Self::hide_kb`] for why this exists rather than a bare setter.
    ///
    /// Unlike hiding, this **rejects** a base that is not installed: "make sure
    /// this is not in my set" is satisfiable for a base that does not exist,
    /// "make sure this *is* in my set" is not, and a caller granting a session
    /// access to a named base wants to hear that the name was wrong rather than
    /// succeed against nothing.
    pub fn include_kb(
        &self,
        session_id: Option<&str>,
        kb_id: &str,
        primary: PrimaryUpdate<'_>,
    ) -> anyhow::Result<KbSelection> {
        let _lock = self.lock_root()?;
        crate::knowledge::paths::validate_kb_id(kb_id)?;
        let installed = self.installed_kb_ids_unlocked()?;
        if !installed.iter().any(|id| id == kb_id) {
            let available = if installed.is_empty() {
                "none".to_string()
            } else {
                installed.join(", ")
            };
            anyhow::bail!("knowledge base '{kb_id}' does not exist (installed: {available})");
        }
        let hidden = self
            .hidden_for_scope_unlocked(session_id)?
            .into_iter()
            .filter(|id| id != kb_id)
            .collect::<Vec<_>>();
        self.apply_selection_unlocked(session_id, Some(&hidden), primary)
    }

    /// Set this scope's set from the ids that should be **visible** — the
    /// inverse of the stored `hidden_kbs`, and the shape every UI actually has
    /// (a list of checked bases), so no caller has to invert it by hand against
    /// a base list it read separately.
    ///
    /// The set is taken literally: any installed base absent from `visible`
    /// becomes hidden, including one created since the caller last listed. Ids
    /// that are not installed are simply not part of the set and are dropped —
    /// there is nothing to hide or show.
    pub fn set_visible_kbs(
        &self,
        session_id: Option<&str>,
        visible: &[String],
        primary: PrimaryUpdate<'_>,
    ) -> anyhow::Result<KbSelection> {
        let _lock = self.lock_root()?;
        let visible = Self::sanitize_kb_id_list(visible)?;
        let hidden = self
            .installed_kb_ids_unlocked()?
            .into_iter()
            .filter(|id| !visible.contains(id))
            .collect::<Vec<_>>();
        self.apply_selection_unlocked(session_id, Some(&hidden), primary)
    }

    /// The engine behind every selection write: decide, validate, *then* write.
    ///
    /// The two halves are strictly ordered and that ordering is the whole
    /// point. This used to write the hidden list first and validate the
    /// requested primary afterwards, so "hide the base I am pinned to, and pin
    /// one that does not exist" persisted the hide, returned an error, and left
    /// the stored pointer sitting outside the resulting set — a half-applied
    /// request that broke the model's one invariant. Nothing below the
    /// "commit" line can fail on anything but I/O.
    ///
    /// Callers must already hold the root lock.
    fn apply_selection_unlocked(
        &self,
        session_id: Option<&str>,
        hidden: Option<&[String]>,
        primary: PrimaryUpdate<'_>,
    ) -> anyhow::Result<KbSelection> {
        // ---- decide: touch nothing on disk until every branch has succeeded ----
        let installed = self.installed_kb_ids_unlocked()?;
        let next_hidden = match hidden {
            Some(ids) => Self::sanitize_kb_id_list(ids)?,
            None => self.hidden_for_scope_unlocked(session_id)?,
        };
        let next_ids = installed
            .iter()
            .filter(|id| !next_hidden.contains(id))
            .cloned()
            .collect::<Vec<_>>();

        let primary_path = self.primary_path_for_scope(session_id);
        let own_primary = self.read_primary_file_unlocked(&primary_path)?;
        let effective_primary = self.effective_primary_unlocked(&own_primary, session_id)?;

        // `None` means "leave this scope's primary file exactly as it is".
        let next_primary: Option<StoredPrimary> = match primary {
            PrimaryUpdate::Unchanged => Self::repair_decision(
                &own_primary,
                &effective_primary,
                &installed,
                &next_ids,
                session_id,
            ),
            PrimaryUpdate::Clear => Some(no_primary_for(session_id)),
            PrimaryUpdate::Inherit => Some(StoredPrimary::Inherit),
            PrimaryUpdate::Set(id) => {
                if !next_ids.iter().any(|known| known == id) {
                    let available = if next_ids.is_empty() {
                        "none".to_string()
                    } else {
                        next_ids.join(", ")
                    };
                    // Scope-appropriate vocabulary: the CLI and scheduled jobs
                    // pass `None` and have no session concept at all (D11), so
                    // telling them to "add it to the session" names nothing
                    // they can act on.
                    match session_id {
                        Some(_) => anyhow::bail!(
                            "knowledge base '{id}' is not one of this session's knowledge bases \
                             ({available}). Add it to the session first, or pass kb_id \
                             explicitly to read it once."
                        ),
                        None => anyhow::bail!(
                            "knowledge base '{id}' is not available ({available}): it does not \
                             exist, or it is hidden."
                        ),
                    }
                }
                Some(StoredPrimary::Pinned(id.to_string()))
            }
        };

        // ---- commit ----
        if hidden.is_some() {
            let path = self.hidden_path_for_scope(session_id);
            self.write_hidden_file_unlocked(&path, &next_hidden)?;
        }
        if let Some(value) = &next_primary {
            self.write_primary_file_unlocked(&primary_path, value)?;
        }

        // Report what we just decided rather than re-reading it: the values are
        // known-coherent here, and a re-read would be three more chances to
        // observe someone else's half-finished edit.
        let effective = match next_primary {
            // Just written: this scope now defers upwards, so what governs is
            // the machine pointer and not the one it held a moment ago.
            Some(StoredPrimary::Inherit) if session_id.is_some() => self
                .read_primary_file_unlocked(&crate::knowledge::paths::primary_kb_path(
                    self.root(),
                ))?,
            Some(settled) => settled,
            None => effective_primary,
        };
        let primary_kb = effective
            .pinned()
            .filter(|id| next_ids.iter().any(|known| known == id))
            .map(ToOwned::to_owned);

        Ok(KbSelection {
            kb_ids: next_ids,
            hidden_kbs: next_hidden,
            primary_kb,
        })
    }
}

impl KnowledgeService {
    pub fn list_history(
        &self,
        kb_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::knowledge::types::HistoryEntry>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        repo.log(limit)
    }

    pub fn restore_state(&self, kb_id: &str, commit_sha: &str) -> anyhow::Result<String> {
        let _lock = FileLockGuard::acquire(&self.kb_lock_path(kb_id))?;
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        let summary = format!("restore to {}", commit_sha.get(..7).unwrap_or(commit_sha));
        let sha = repo.restore_to(commit_sha, &summary)?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(sha)
    }

    pub fn preview_state(
        &self,
        kb_id: &str,
        commit_sha: &str,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let repo = GitRepo::open(&kb_root)?;
        repo.read_file_at(commit_sha, path)
    }
}

impl KnowledgeService {
    /// Re-run credibility classification for an existing raw source using the stored URL or
    /// the derived markdown text (for File/Text sources) and persist the result to `meta.yaml`.
    pub async fn reclassify_source(
        &self,
        kb_id: &str,
        source_id: &str,
    ) -> anyhow::Result<crate::knowledge::types::Credibility> {
        let _lock = FileLockGuard::acquire(&self.kb_lock_path(kb_id))?;
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let mut meta = raw::read_meta(&kb_root, source_id)?;

        // The stored, already-extracted text is our best probe for identifiers
        // (DOI / journal markers) — read it once and feed it to the classifier
        // regardless of source kind so re-running classification on an old,
        // mislabelled source can now recover its true peer-reviewed tier.
        let stored_body =
            std::fs::read_to_string(kb_root.join("raw").join(source_id).join("source.md")).ok();

        // Reconstruct a SourceInput from what was stored.  URL-based sources keep the url;
        // everything else falls back to the derived markdown (source.md).
        let input = if let Some(url) = meta.url.clone() {
            convert::SourceInput::Url(url)
        } else {
            convert::SourceInput::Text {
                text: stored_body.clone().unwrap_or_default(),
                title: Some(meta.title.clone()),
            }
        };

        let new_cred =
            credibility::classify_with_text(&input, stored_body.as_deref(), None).await?;
        meta.credibility = new_cred.clone();
        let yaml = serde_yaml::to_string(&meta)?;
        std::fs::write(kb_root.join("raw").join(source_id).join("meta.yaml"), yaml)?;

        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("reclassify {source_id}"),
            None,
        )?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(new_cred)
    }

    /// Write a manually-specified `Credibility` override to `meta.yaml` and commit.
    /// Returns the credibility that was stored (same as input).
    pub fn override_credibility(
        &self,
        kb_id: &str,
        source_id: &str,
        cred: crate::knowledge::types::Credibility,
    ) -> anyhow::Result<crate::knowledge::types::Credibility> {
        let _lock = FileLockGuard::acquire(&self.kb_lock_path(kb_id))?;
        paths::validate_kb_id(kb_id)?;
        let kb_root = paths::kb_root(&self.root, kb_id);
        let mut meta = raw::read_meta(&kb_root, source_id)?;
        meta.credibility = cred.clone();
        let yaml = serde_yaml::to_string(&meta)?;
        std::fs::write(kb_root.join("raw").join(source_id).join("meta.yaml"), yaml)?;
        let repo = GitRepo::open(&kb_root)?;
        repo.commit_all(
            crate::knowledge::types::ChangeKind::Manual,
            &format!("override credibility for {source_id}"),
            None,
        )?;
        self.rebuild_graph_cache(kb_id)?;
        Ok(cred)
    }
}

impl KnowledgeService {
    /// Perform a trivial LLM completion to verify the provider is reachable.
    ///
    /// Sends a single "Reply with OK" message to the model.  Any network error,
    /// authentication failure, or invalid model name surfaces as `Err`.
    pub async fn check_model(
        &self,
        completer: Box<dyn crate::knowledge::subagent::loop_::Completer>,
    ) -> anyhow::Result<()> {
        let messages = vec![crate::knowledge::subagent::loop_::LlmMessage::User(
            "Reply with exactly OK and nothing else.".to_string(),
        )];
        completer
            .complete(
                "You are a connectivity test. Respond with OK.",
                &messages,
                &[], // no tools
            )
            .await
            .map_err(|e| anyhow::anyhow!("LLM check failed: {e}"))?;
        Ok(())
    }
}

/// True when `name` is a session-digest filename — 64 lowercase hex chars,
/// exactly what `raw::hash_bytes` produces. Everything else in a
/// `.*-sessions/` directory is debris (most often a `<digest>.tmp` staged
/// write that a crash left behind) and must never be read as session state.
fn is_session_digest(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.len() == 64 && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::convert::SourceInput;
    use crate::knowledge::types::{ChangeKind, Credibility, CredibilityTier};

    fn svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        (dir, svc)
    }

    /// Issue #56 DR-26 / Task 50, and the regression that made it necessary.
    ///
    /// This module is the whole production surface of the tier ratchet: every
    /// other caller in the tree goes through `KnowledgeService::raise_tier` or is
    /// a test. Task 50 shipped with three `raise_unlocked` call sites here and an
    /// affiliation twin on only one, so `kb_create_base` and `kb_import` moved
    /// the tier and left the base reading UNCLAIMED — which
    /// `affiliation::reachable` treats as permissive for every private model. A
    /// UCSF chat could export a base it owns and any other institution's chat
    /// could import the archive and read it, both endpoints Private, no gate
    /// crossed.
    ///
    /// ⚠ **A tripwire over one spelling, not a proof** — the same shape as
    /// `tier_user::tests::exactly_one_writer_outside_the_ratchet_saves_the_tier_store`.
    /// What it reliably catches is the realistic case: a third ratchet path
    /// added here that stamps the tier and forgets the third axis. The two
    /// permitted sites are [`KnowledgeService::raise_tier_and_affiliation`] (for
    /// a base that already exists) and [`KnowledgeService::stamp_new_base_unlocked`]
    /// (for one this call is minting); each does both raises itself, so there is
    /// no third function that could do one.
    #[test]
    fn the_tier_ratchet_has_no_production_call_site_that_skips_the_affiliation() {
        let this =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/knowledge/service.rs");
        let src = std::fs::read_to_string(&this)
            .unwrap_or_else(|e| panic!("the audit could not read {}: {e}", this.display()));
        // Composed, so the audit does not match itself.
        let needle = concat!("tier::", "raise_unlocked(");
        let sites: Vec<&str> = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && l.contains(needle))
            .collect();
        assert_eq!(
            sites.len(),
            2,
            "the tier ratchet is called {} times in service.rs, not 2. Every \
             production raise must be paired with the affiliation raise in the \
             same function: use `stamp_new_base_unlocked` for a base this call \
             is minting, or `raise_tier_and_affiliation` for one that already \
             exists. Sites found: {sites:#?}",
            sites.len()
        );
        assert_eq!(
            src.matches(concat!("tier::", "add_owners_unlocked("))
                .count()
                + src
                    .matches(concat!("tier::", "raise_affiliation_unlocked("))
                    .count(),
            2,
            "the affiliation ratchet is reached from somewhere new. It must be \
             reached from exactly the two functions the tier ratchet is, and \
             from the same line of each: `raise_tier_and_affiliation` and \
             `stamp_new_base_unlocked`."
        );
    }

    #[test]
    fn create_base_writes_all_files_and_inits_git() {
        let (_dir, svc) = svc();
        let m = svc.create_base("ms", "MS Patient Analysis", None).unwrap();
        let kb = svc.root().join("ms");
        assert!(kb.join("manifest.yaml").exists());
        assert!(kb.join("schema.md").exists());
        assert!(kb.join("index.md").exists());
        assert!(kb.join("log.md").exists());
        assert!(kb.join(".gitignore").exists());
        assert!(kb.join("knowledge/entities").exists());
        assert!(kb.join("knowledge/concepts").exists());
        assert!(kb.join("knowledge/sources").exists());
        assert!(kb.join("knowledge/notes").exists());
        assert!(kb.join("raw").exists());
        assert!(kb.join(".biorouter-knowledge").exists());
        assert!(kb.join(".git").exists());
        assert_eq!(m.id, "ms");

        // Initial commit exists.
        let repo = GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].summary.contains("create knowledge base ms"));

        // Registry has one entry.
        let bases = svc.list_bases().unwrap();
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].name, "MS Patient Analysis");
    }

    #[test]
    fn create_base_rejects_duplicate() {
        let (_dir, svc) = svc();
        svc.create_base("ms", "x", None).unwrap();
        let err = svc.create_base("ms", "y", None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn create_base_rejects_invalid_id() {
        let (_dir, svc) = svc();
        let err = svc.create_base("BAD", "x", None).unwrap_err();
        assert!(err.to_string().contains("a-z, 0-9"), "got: {err}");
    }

    #[tokio::test]
    async fn add_raw_source_from_text() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        let res = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "Lab note: HRV trend up after week of zone-2.".into(),
                    title: Some("HRV note".into()),
                },
                None,
            )
            .await
            .unwrap();

        assert!(kb.join(format!("raw/{}/source.md", res.source_id)).exists());
        assert!(kb.join(format!("raw/{}/meta.yaml", res.source_id)).exists());
        let meta = raw::read_meta(&kb, &res.source_id).unwrap();
        assert_eq!(meta.title, "HRV note");
        assert_eq!(meta.credibility.tier, CredibilityTier::Personal);

        // A commit was made.
        let repo = GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 2, "create + add_raw_source");
        assert_eq!(log[0].kind, ChangeKind::Ingest);
    }

    #[tokio::test]
    async fn add_raw_source_from_html_file() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let html = b"<html><head><title>Test</title></head><body><h1>H</h1></body></html>";
        let res = svc
            .add_raw_source(
                "k",
                SourceInput::File {
                    bytes: html.to_vec(),
                    filename: "x.html".into(),
                    mime: Some("text/html".into()),
                },
                None,
            )
            .await
            .unwrap();
        let kb = svc.root().join("k");
        let md =
            std::fs::read_to_string(kb.join(format!("raw/{}/source.md", res.source_id))).unwrap();
        assert!(md.contains("# H"));
    }

    #[test]
    fn looks_machine_generated_detects_uuids_and_hashes() {
        assert!(looks_machine_generated(
            "a64e171e-f161-4615-9299-839c8a066049.pdf"
        ));
        assert!(looks_machine_generated(
            "a64e171e-f161-4615-9299-839c8a066049-pdf-48f040"
        ));
        assert!(looks_machine_generated("deadbeefdeadbeefdeadbeef"));
        assert!(!looks_machine_generated(
            "Effects of e-cigarette aerosol inhalation in mice"
        ));
        assert!(!looks_machine_generated("RNA-seq"));
    }

    #[test]
    fn title_from_markdown_prefers_first_heading() {
        let md =
            "> **Warning: poor extraction quality.**\n\n# A Study of Airway Deposition\n\nbody";
        assert_eq!(
            title_from_markdown(md).as_deref(),
            Some("A Study of Airway Deposition")
        );
    }

    #[tokio::test]
    async fn add_raw_source_rescues_uuid_filename_title_from_body() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        // An HTML upload whose filename is a UUID but whose body has a real title.
        let html = b"<html><body><h1>Intersubject Variability in Aerosol Deposition</h1>\
                     <p>Long body text describing the study in detail.</p></body></html>";
        let res = svc
            .add_raw_source(
                "k",
                SourceInput::File {
                    bytes: html.to_vec(),
                    filename: "a64e171e-f161-4615-9299-839c8a066049.html".into(),
                    mime: Some("text/html".into()),
                },
                None,
            )
            .await
            .unwrap();
        let kb = svc.root().join("k");
        let meta = raw::read_meta(&kb, &res.source_id).unwrap();
        assert!(
            !meta.title.contains("a64e171e"),
            "title should not be the UUID filename, got {}",
            meta.title
        );
        assert!(
            meta.title.to_lowercase().contains("aerosol"),
            "got {}",
            meta.title
        );
    }

    #[tokio::test]
    async fn add_raw_source_reuses_existing_source_for_same_text_hash() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let first = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "Same content".into(),
                    title: Some("First note".into()),
                },
                None,
            )
            .await
            .unwrap();

        let second = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "Same content".into(),
                    title: Some("Second note".into()),
                },
                None,
            )
            .await
            .unwrap();

        assert_eq!(first.source_id, second.source_id);

        let repo = GitRepo::open(&svc.root().join("k")).unwrap();
        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 2, "create + first add_raw_source only");
    }

    #[tokio::test]
    async fn get_graph_returns_cached_after_create_and_add() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let g_empty = svc.get_graph("k").unwrap();
        assert!(g_empty.nodes.is_empty());
        svc.add_raw_source(
            "k",
            convert::SourceInput::Text {
                text: "note".into(),
                title: Some("N".into()),
            },
            None,
        )
        .await
        .unwrap();
        let kb = svc.root().join("k");
        // Source pages aren't written by add_raw_source — only raw/. So the graph
        // remains empty until a macro creates knowledge/sources/<id>.md (Plan 2).
        let g = svc.get_graph("k").unwrap();
        assert_eq!(g.nodes.len(), 0, "no knowledge pages yet");
        assert!(kb.join(".biorouter-knowledge/graph-cache.json").exists());
    }

    /// `GITIGNORE` is a file body, so it cannot be built from
    /// [`paths::KB_WRITE_LOCK_REL`] at compile time, which leaves it the one
    /// place the lock's path is still spelled by hand. This is the seam that
    /// closes it: rename the lock and the const stops covering it, silently,
    /// and the transient file starts appearing in every KB's git history.
    #[test]
    fn the_gitignore_still_names_the_write_lock_it_is_meant_to_hide() {
        assert!(
            GITIGNORE
                .lines()
                .any(|line| line == paths::KB_WRITE_LOCK_REL),
            "GITIGNORE does not ignore {}: {GITIGNORE:?}",
            paths::KB_WRITE_LOCK_REL
        );
    }

    #[tokio::test]
    async fn lock_kb_serializes_writers() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let svc1 = svc.clone();
        let svc2 = svc.clone();
        let h1 = tokio::spawn(async move {
            let _g = svc1.lock_kb("k").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            std::time::Instant::now()
        });
        // Brief delay so h1 acquires the lock first.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let h2 = tokio::spawn(async move {
            let _g = svc2.lock_kb("k").await.unwrap();
            std::time::Instant::now()
        });
        let t1 = h1.await.unwrap();
        let t2 = h2.await.unwrap();
        assert!(
            t2 >= t1,
            "h2 must observe lock acquisition after h1 released"
        );
    }

    #[tokio::test]
    async fn reclassify_source_updates_meta() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();

        // Add a text source (no URL → falls back to Personal tier).
        let written = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "lab note".into(),
                    title: Some("note".into()),
                },
                None,
            )
            .await
            .unwrap();

        // Reclassify — same text, should still come back Personal.
        let cred = svc
            .reclassify_source("k", &written.source_id)
            .await
            .unwrap();
        assert_eq!(cred.tier, CredibilityTier::Personal);

        // Verify meta.yaml was updated.
        let kb = svc.root().join("k");
        let meta = raw::read_meta(&kb, &written.source_id).unwrap();
        assert_eq!(meta.credibility.tier, CredibilityTier::Personal);

        // A new commit was made.
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        // create + add_raw + reclassify = 3 commits
        assert!(log.len() >= 3);
    }

    #[tokio::test]
    async fn override_credibility_writes_and_commits() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();

        let written = svc
            .add_raw_source(
                "k",
                SourceInput::Text {
                    text: "draft".into(),
                    title: Some("draft".into()),
                },
                None,
            )
            .await
            .unwrap();

        let override_cred = Credibility {
            tier: CredibilityTier::PeerReviewed,
            confidence: 0.99,
            publisher: Some("Nature".into()),
            venue: Some("Nature 2024".into()),
            doi: Some("10.1000/xyz".into()),
            retracted: false,
            reasoning: "Manual override: confirmed peer-reviewed publication.".into(),
            classifier_version: 1,
        };

        let returned = svc
            .override_credibility("k", &written.source_id, override_cred.clone())
            .unwrap();
        assert_eq!(returned.tier, CredibilityTier::PeerReviewed);
        assert_eq!(returned.doi.as_deref(), Some("10.1000/xyz"));

        // Verify meta.yaml persisted the override.
        let kb = svc.root().join("k");
        let meta = raw::read_meta(&kb, &written.source_id).unwrap();
        assert_eq!(meta.credibility.tier, CredibilityTier::PeerReviewed);
        assert_eq!(meta.credibility.doi.as_deref(), Some("10.1000/xyz"));

        // A commit was made with the override.
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        let log = repo.log(10).unwrap();
        assert!(log[0].summary.contains("override credibility"));
        assert_eq!(log[0].kind, ChangeKind::Manual);
    }

    // -----------------------------------------------------------------------
    // check_model tests
    // -----------------------------------------------------------------------

    use crate::knowledge::subagent::loop_::{Completer, LlmMessage, LlmReply};
    use async_trait::async_trait;
    use rmcp::model::Tool;

    struct OkCompleter;

    #[async_trait]
    impl Completer for OkCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            Ok(LlmReply {
                text: "OK".to_string(),
                tool_calls: vec![],
            })
        }
    }

    struct ErrCompleter;

    #[async_trait]
    impl Completer for ErrCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            anyhow::bail!("provider unreachable")
        }
    }

    #[tokio::test]
    async fn check_model_ok_with_mock_completer() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.check_model(Box::new(OkCompleter)).await.unwrap();
    }

    #[tokio::test]
    async fn check_model_propagates_completer_error() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        let err = svc.check_model(Box::new(ErrCompleter)).await.unwrap_err();
        assert!(
            err.to_string().contains("provider unreachable"),
            "error should mention underlying cause, got: {err}"
        );
    }

    #[tokio::test]
    async fn list_history_and_restore_roundtrip() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        svc.add_raw_source(
            "k",
            convert::SourceInput::Text {
                text: "first".into(),
                title: Some("a".into()),
            },
            None,
        )
        .await
        .unwrap();
        let history_after_one = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_one.len(), 2);
        let target = history_after_one.last().unwrap().commit_sha.clone();

        svc.add_raw_source(
            "k",
            convert::SourceInput::Text {
                text: "second".into(),
                title: Some("b".into()),
            },
            None,
        )
        .await
        .unwrap();
        let history_after_two = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_two.len(), 3);

        svc.restore_state("k", &target).unwrap();
        let history_after_restore = svc.list_history("k", 10).unwrap();
        assert_eq!(history_after_restore.len(), 4);
        assert_eq!(
            history_after_restore[0].kind,
            crate::knowledge::types::ChangeKind::Restore
        );
    }

    #[test]
    fn primary_kb_persists_to_disk() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        assert!(svc.get_primary_persisted()?.is_none());

        svc.set_primary_persisted(Some("my-kb"))?;
        assert_eq!(svc.get_primary_persisted()?.as_deref(), Some("my-kb"));

        // Setting again overwrites.
        svc.set_primary_persisted(Some("other-kb"))?;
        assert_eq!(svc.get_primary_persisted()?.as_deref(), Some("other-kb"));

        // Clearing removes the file.
        svc.set_primary_persisted(None)?;
        assert!(svc.get_primary_persisted()?.is_none());

        // Invalid IDs are rejected.
        let err = svc.set_primary_persisted(Some("INVALID--KB"));
        assert!(err.is_err());
        Ok(())
    }

    #[test]
    fn primary_kb_can_be_scoped_per_session() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());

        assert!(svc.get_primary_for_session("session-a")?.is_none());
        assert!(svc.get_primary_for_session("session-b")?.is_none());

        svc.set_primary_for_session("session-a", Some("kb-a"))?;
        svc.set_primary_for_session("session-b", Some("kb-b"))?;

        assert_eq!(
            svc.get_primary_for_session("session-a")?.as_deref(),
            Some("kb-a")
        );
        assert_eq!(
            svc.get_primary_for_session("session-b")?.as_deref(),
            Some("kb-b")
        );

        svc.set_primary_for_session("session-a", None)?;
        assert!(svc.get_primary_for_session("session-a")?.is_none());
        assert_eq!(
            svc.get_primary_for_session("session-b")?.as_deref(),
            Some("kb-b")
        );

        Ok(())
    }

    #[test]
    fn session_scoped_primary_kb_tracks_rename_and_delete() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());

        svc.create_base("kb-a", "KB A", None)?;
        svc.set_primary_persisted(Some("kb-a"))?;
        svc.set_primary_for_session("session-a", Some("kb-a"))?;
        svc.set_primary_for_session("session-b", Some("kb-a"))?;

        let renamed = svc.update_base("kb-a", Some("Renamed KB"), None)?;
        assert_eq!(renamed.id, "renamed-kb");
        assert_eq!(svc.get_primary_persisted()?.as_deref(), Some("renamed-kb"));
        assert_eq!(
            svc.get_primary_for_session("session-a")?.as_deref(),
            Some("renamed-kb")
        );
        assert_eq!(
            svc.get_primary_for_session("session-b")?.as_deref(),
            Some("renamed-kb")
        );

        svc.delete_base("renamed-kb")?;
        assert!(svc.get_primary_persisted()?.is_none());
        assert!(svc.get_primary_for_session("session-a")?.is_none());
        assert!(svc.get_primary_for_session("session-b")?.is_none());

        Ok(())
    }

    #[test]
    fn hidden_kbs_can_be_scoped_per_session() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());

        assert!(svc.get_hidden_persisted()?.is_empty());
        assert!(svc.get_hidden_for_session("session-a")?.is_empty());

        svc.set_hidden_persisted(&["kb-a".to_string(), "kb-b".to_string()])?;
        assert_eq!(
            svc.get_hidden_for_session_or_persisted("session-a")?,
            vec!["kb-a".to_string(), "kb-b".to_string()]
        );

        svc.set_hidden_for_session("session-a", &["kb-c".to_string()])?;
        assert_eq!(
            svc.get_hidden_for_session("session-a")?,
            vec!["kb-c".to_string()]
        );
        assert_eq!(
            svc.get_hidden_for_session_or_persisted("session-a")?,
            vec!["kb-c".to_string()]
        );
        assert_eq!(
            svc.get_hidden_for_session_or_persisted("session-b")?,
            vec!["kb-a".to_string(), "kb-b".to_string()]
        );

        // Setting an empty list is an explicit override ("hide nothing here"),
        // not a request to fall back to the machine-wide list. See
        // `session_hidden_override_can_be_explicitly_empty`.
        svc.set_hidden_for_session("session-a", &[])?;
        assert!(svc.get_hidden_for_session("session-a")?.is_empty());
        assert!(svc
            .get_hidden_for_session_or_persisted("session-a")?
            .is_empty());

        Ok(())
    }

    /// The session's knowledge-base *set* — the one axis. Sorted, so any
    /// "first member" rule downstream is stable across processes and
    /// independent of registry insertion order.
    #[test]
    fn session_kb_ids_are_the_visible_set_sorted() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("zulu", "Zulu", None)?;
        svc.create_base("alpha", "Alpha", None)?;
        svc.create_base("mike", "Mike", None)?;

        // No session in scope (the CLI, a scheduled job): the machine list applies.
        svc.set_hidden_persisted(&["mike".to_string()])?;
        assert_eq!(
            svc.session_kb_ids(None)?,
            vec!["alpha".to_string(), "zulu".to_string()]
        );

        // A session override replaces the machine list wholesale, never unions.
        svc.set_hidden_for_session("session-a", &["zulu".to_string()])?;
        assert_eq!(
            svc.session_kb_ids(Some("session-a"))?,
            vec!["alpha".to_string(), "mike".to_string()]
        );

        // A session that never overrode inherits.
        assert_eq!(
            svc.session_kb_ids(Some("session-b"))?,
            vec!["alpha".to_string(), "zulu".to_string()]
        );
        Ok(())
    }

    /// Under the merged model the hidden list *is* the session's set, so
    /// "everything is in this chat" is the most common gesture there is. It
    /// must be a state the store can hold — writing an empty list used to
    /// delete the override file, and `get_hidden_for_session_or_persisted`
    /// uses file existence as its discriminator, so the session silently
    /// re-inherited the machine-wide list.
    #[test]
    fn session_hidden_override_can_be_explicitly_empty() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.set_hidden_persisted(&["kb-a".to_string()])?;

        // "Show everything in this chat" must NOT re-inherit the machine list.
        svc.set_hidden_for_session("session-a", &[])?;
        assert!(
            svc.get_hidden_for_session_or_persisted("session-a")?
                .is_empty(),
            "an explicitly empty session override must not inherit the machine default"
        );

        // A session that never overrode still inherits.
        assert_eq!(
            svc.get_hidden_for_session_or_persisted("session-b")?,
            vec!["kb-a".to_string()]
        );

        // Dropping the override is a separate, explicit gesture.
        svc.clear_hidden_for_session("session-a")?;
        assert_eq!(
            svc.get_hidden_for_session_or_persisted("session-a")?,
            vec!["kb-a".to_string()]
        );
        Ok(())
    }

    /// The one invariant of the merged model: the primary is always a member
    /// of the session's set. Enforced on the read side (never return a
    /// non-member) and on the write side (repair, and persist the repair, when
    /// a set change orphans it). It is never *invented* — a session with bases
    /// but no chosen primary has none, so a KB-less write fails loudly instead
    /// of landing in whichever base happens to sort first.
    #[test]
    fn primary_must_be_a_member_of_the_session_set() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("alpha", "Alpha", None)?;
        svc.create_base("beta", "Beta", None)?;
        svc.create_base("gamma", "Gamma", None)?;

        // Never invented.
        assert_eq!(svc.primary_for_session(Some("session-a"))?, None);

        svc.set_primary_for_session("session-a", Some("beta"))?;
        assert_eq!(
            svc.primary_for_session(Some("session-a"))?.as_deref(),
            Some("beta")
        );

        // Hiding the primary from this chat promotes to the lexicographically
        // first remaining member — and persists it, so the CLI and the GUI see
        // the same answer as the model.
        svc.set_hidden_for_session("session-a", &["beta".to_string()])?;
        assert_eq!(
            svc.primary_for_session(Some("session-a"))?.as_deref(),
            Some("alpha")
        );
        assert_eq!(
            svc.get_primary_for_session("session-a")?.as_deref(),
            Some("alpha"),
            "the promotion must be persisted, not re-derived on every read"
        );

        // Hiding everything clears it.
        svc.set_hidden_for_session(
            "session-a",
            &["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
        )?;
        assert_eq!(svc.primary_for_session(Some("session-a"))?, None);
        assert_eq!(svc.get_primary_for_session("session-a")?, None);

        // A machine-wide primary is inherited by a session that has not chosen
        // one — but only while that session actually holds the base.
        svc.set_primary_persisted(Some("gamma"))?;
        assert_eq!(
            svc.primary_for_session(Some("session-b"))?.as_deref(),
            Some("gamma")
        );
        svc.set_hidden_for_session("session-b", &["gamma".to_string()])?;
        assert_eq!(
            svc.primary_for_session(Some("session-b"))?.as_deref(),
            Some("alpha"),
            "hiding the inherited primary promotes inside the session, exactly as \
             hiding a pinned one does"
        );
        assert_eq!(
            svc.get_primary_persisted()?.as_deref(),
            Some("gamma"),
            "and it leaves the machine pointer alone for every other chat"
        );
        Ok(())
    }

    /// Two chats, one gesture, one answer. A chat that *pinned* alpha and a
    /// chat that *inherited* alpha from the machine pointer show the user the
    /// same thing — "alpha is this chat's primary" — so hiding alpha must land
    /// them in the same place.
    ///
    /// It did not. `repair_decision` returned early unless the scope's own file
    /// was `Pinned`, and an inheriting session's own file is `Inherit`, so the
    /// repair never saw the pointer it was meant to repair: the pinning chat
    /// came back promoted to beta, the inheriting chat came back with no
    /// primary at all. The inheriting case is the common one — most chats never
    /// pin their own primary — so the divergence was the default experience.
    #[test]
    fn hiding_the_primary_promotes_whether_the_chat_pinned_or_inherited_it() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        for id in ["alpha", "beta", "gamma"] {
            svc.create_base(id, id, None)?;
        }
        svc.set_primary_persisted(Some("alpha"))?;
        svc.set_primary_for_session("pinned", Some("alpha"))?;

        // Identical starting state as far as the user can tell.
        assert_eq!(
            svc.primary_for_session(Some("pinned"))?.as_deref(),
            Some("alpha")
        );
        assert_eq!(
            svc.primary_for_session(Some("inherits"))?.as_deref(),
            Some("alpha")
        );

        let pinned = svc.hide_kb(Some("pinned"), "alpha", PrimaryUpdate::Unchanged)?;
        let inherits = svc.hide_kb(Some("inherits"), "alpha", PrimaryUpdate::Unchanged)?;
        assert_eq!(
            pinned.primary_kb, inherits.primary_kb,
            "the same gesture from the same visible state must give the same primary"
        );
        assert_eq!(inherits.primary_kb.as_deref(), Some("beta"));

        // The promotion is persisted as this chat's *own* pin: it has diverged
        // from the machine default, which is what a session pin means.
        assert_eq!(
            svc.get_primary_for_session("inherits")?.as_deref(),
            Some("beta"),
            "the promotion must be persisted, not re-derived on every read"
        );
        // And the machine pointer is untouched, so every other chat still
        // follows it.
        assert_eq!(svc.get_primary_persisted()?.as_deref(), Some("alpha"));
        assert_eq!(
            svc.primary_for_session(Some("bystander"))?.as_deref(),
            Some("alpha")
        );

        // Hiding the rest leaves the chat with no primary — and it stays gone
        // rather than re-inheriting the machine default it has moved away from,
        // and is never re-invented when a base comes back into the set.
        let emptied = svc.set_visible_kbs(Some("inherits"), &[], PrimaryUpdate::Unchanged)?;
        assert_eq!(emptied.primary_kb, None);
        let restored = svc.include_kb(Some("inherits"), "gamma", PrimaryUpdate::Unchanged)?;
        assert_eq!(
            restored.primary_kb, None,
            "a chat that has been left with no primary must not be handed one"
        );
        Ok(())
    }

    /// Both session directories are staged through `<digest>.tmp` in the same
    /// directory they are read from, so a crash between write and rename
    /// leaves a file the rewriters used to treat as a live session: the
    /// primary rewriter edited it, and a torn hidden leftover made the whole
    /// rename fail with `?` — a crash last week surfacing as "rename knowledge
    /// base" erroring today.
    #[test]
    fn session_rewriters_skip_crash_leftover_tmp_files() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("kb-a", "KB A", None)?;
        svc.set_primary_for_session("session-live", Some("kb-a"))?;
        svc.set_hidden_for_session("session-live", &[])?;

        let leftover = format!("{}.tmp", crate::knowledge::raw::hash_bytes(b"session-dead"));
        let primary_tmp =
            crate::knowledge::paths::primary_kb_sessions_dir(svc.root()).join(&leftover);
        std::fs::write(&primary_tmp, b"kb-a")?;
        let hidden_tmp =
            crate::knowledge::paths::hidden_kb_sessions_dir(svc.root()).join(&leftover);
        std::fs::write(&hidden_tmp, b"half-written garbage")?;

        // A rename must succeed and must rewrite only the live session files.
        let renamed = svc.update_base("kb-a", Some("Renamed KB"), None)?;
        assert_eq!(renamed.id, "renamed-kb");
        assert_eq!(
            svc.get_primary_for_session("session-live")?.as_deref(),
            Some("renamed-kb"),
            "the live session file must still be rewritten"
        );
        assert_eq!(
            std::fs::read_to_string(&primary_tmp)?,
            "kb-a",
            "a crash leftover is not a session"
        );
        assert_eq!(
            std::fs::read_to_string(&hidden_tmp)?,
            "half-written garbage"
        );
        Ok(())
    }

    /// One request, one lock, validated against the *resulting* set — so
    /// "add this base to the chat and make it primary" is expressible, and a
    /// set-only edit can never move the pointer.
    #[test]
    fn set_selection_applies_set_and_primary_as_one_operation() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        for id in ["alpha", "beta", "gamma"] {
            svc.create_base(id, id, None)?;
        }
        svc.set_hidden_for_session("session-a", &["beta".to_string()])?;

        let sel = svc.set_selection(Some("session-a"), Some(&[]), PrimaryUpdate::Set("beta"))?;
        assert_eq!(
            sel.kb_ids,
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
        assert_eq!(sel.primary_kb.as_deref(), Some("beta"));

        let sel = svc.set_selection(
            Some("session-a"),
            Some(&["gamma".to_string()]),
            PrimaryUpdate::Unchanged,
        )?;
        assert_eq!(sel.kb_ids, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(
            sel.primary_kb.as_deref(),
            Some("beta"),
            "a set-only edit must not move the pointer"
        );

        let err = svc
            .set_selection(Some("session-a"), None, PrimaryUpdate::Set("gamma"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("gamma") && err.contains("alpha, beta"),
            "the rejection must name the id and the set it is not in, got: {err}"
        );

        let sel = svc.set_selection(Some("session-a"), None, PrimaryUpdate::Clear)?;
        assert_eq!(sel.primary_kb, None);
        Ok(())
    }

    /// The membership primitives every caller actually needs, so none of them
    /// has to read the hidden list, edit it and write it back. Each takes the
    /// whole gesture and applies it under one root lock.
    #[test]
    fn membership_primitives_apply_one_gesture_under_one_lock() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        for id in ["alpha", "beta", "gamma"] {
            svc.create_base(id, id, None)?;
        }

        // Hide one base; the pointer is untouched because it was not the one hidden.
        svc.set_selection(Some("s1"), Some(&[]), PrimaryUpdate::Set("alpha"))?;
        let sel = svc.hide_kb(Some("s1"), "gamma", PrimaryUpdate::Unchanged)?;
        assert_eq!(sel.kb_ids, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(sel.hidden_kbs, vec!["gamma".to_string()]);
        assert_eq!(sel.primary_kb.as_deref(), Some("alpha"));

        // Hiding is idempotent, and hiding an uninstalled base is accepted.
        assert_eq!(
            svc.hide_kb(Some("s1"), "gamma", PrimaryUpdate::Unchanged)?,
            sel
        );
        svc.hide_kb(Some("s1"), "ghost", PrimaryUpdate::Unchanged)?;
        assert_eq!(
            svc.selection(Some("s1"))?.kb_ids,
            vec!["alpha".to_string(), "beta".to_string()]
        );

        // Add a base back and make it primary in the same operation — the
        // combined gesture the apps platform and the chat chip both need.
        let sel = svc.include_kb(Some("s1"), "gamma", PrimaryUpdate::Set("gamma"))?;
        assert_eq!(
            sel.kb_ids,
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
        assert_eq!(sel.primary_kb.as_deref(), Some("gamma"));

        // Including a base that does not exist is an error, not a silent no-op.
        let err = svc
            .include_kb(Some("s1"), "ghost", PrimaryUpdate::Unchanged)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("ghost") && err.contains("does not exist"),
            "got: {err}"
        );
        assert_eq!(
            svc.selection(Some("s1"))?,
            sel,
            "a rejected include changes nothing"
        );

        // The visible-set form is the exact inverse of hidden_kbs.
        let sel = svc.set_visible_kbs(
            Some("s1"),
            &["beta".to_string(), "gamma".to_string()],
            PrimaryUpdate::Unchanged,
        )?;
        assert_eq!(sel.kb_ids, vec!["beta".to_string(), "gamma".to_string()]);
        assert_eq!(sel.hidden_kbs, vec!["alpha".to_string()]);
        assert_eq!(
            sel.primary_kb.as_deref(),
            Some("gamma"),
            "the pointer stays put while it is still a member"
        );

        // Dropping the primary out of the visible set repairs it, once.
        let sel =
            svc.set_visible_kbs(Some("s1"), &["beta".to_string()], PrimaryUpdate::Unchanged)?;
        assert_eq!(sel.kb_ids, vec!["beta".to_string()]);
        assert_eq!(sel.primary_kb.as_deref(), Some("beta"));

        // And they validate the primary against the resulting set, all-or-nothing.
        let before = svc.selection(Some("s1"))?;
        assert!(svc
            .set_visible_kbs(
                Some("s1"),
                &["beta".to_string()],
                PrimaryUpdate::Set("alpha")
            )
            .is_err());
        assert_eq!(svc.selection(Some("s1"))?, before);

        // Machine scope works the same way.
        let sel = svc.hide_kb(None, "beta", PrimaryUpdate::Unchanged)?;
        assert_eq!(sel.kb_ids, vec!["alpha".to_string(), "gamma".to_string()]);
        Ok(())
    }

    /// The reason these primitives exist. Hiding four different bases from four
    /// threads used to be four read-modify-write cycles across two unlocked
    /// calls, so each writer persisted a list computed before the others' edits
    /// and updates were silently lost. Every hide must survive.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_hides_of_different_bases_all_survive() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        let ids = ["alpha", "beta", "gamma", "delta"];
        for id in ids {
            svc.create_base(id, id, None)?;
        }

        let mut tasks = Vec::new();
        for id in ids {
            let svc = svc.clone();
            tasks.push(tokio::task::spawn_blocking(
                move || -> anyhow::Result<()> {
                    svc.hide_kb(Some("s1"), id, PrimaryUpdate::Unchanged)?;
                    Ok(())
                },
            ));
        }
        for task in tasks {
            task.await??;
        }

        let sel = svc.selection(Some("s1"))?;
        assert!(
            sel.kb_ids.is_empty(),
            "every concurrent hide must survive, got {:?}",
            sel.kb_ids
        );
        assert_eq!(sel.hidden_kbs.len(), ids.len());
        Ok(())
    }

    /// Repair must tell "deleted" from "hidden". A pointer at a base that is no
    /// longer installed is a dangling reference and must be *cleared*; only a
    /// base that still exists and was merely hidden may promote. Conflating the
    /// two meant an upgrade whose `.active-kb` named a since-removed base read
    /// as no-primary at first and then, on the next entirely unrelated hide,
    /// invented one — the single thing the merged model forbids.
    #[test]
    fn repair_clears_a_stale_pointer_and_promotes_only_a_hidden_one() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        for id in ["alpha", "beta", "gamma"] {
            svc.create_base(id, id, None)?;
        }

        // An upgrade (or an out-of-band edit): the machine pointer names a base
        // that is not installed. It correctly reads as no primary …
        std::fs::write(
            crate::knowledge::paths::primary_kb_path(svc.root()),
            b"ghost",
        )?;
        assert_eq!(svc.primary_for_session(None)?, None);

        // … and an unrelated set edit must not turn it into one.
        let sel =
            svc.set_selection(None, Some(&["gamma".to_string()]), PrimaryUpdate::Unchanged)?;
        assert_eq!(
            sel.primary_kb, None,
            "a primary must never be invented from a dangling pointer"
        );
        assert_eq!(
            svc.get_primary_persisted()?,
            None,
            "the dangling pointer is cleared, not promoted"
        );

        // A pointer at a base that still exists but was hidden *does* promote:
        // the user ranked it, and there is still a set to fall back into.
        svc.set_selection(None, Some(&[]), PrimaryUpdate::Set("beta"))?;
        let sel = svc.set_selection(None, Some(&["beta".to_string()]), PrimaryUpdate::Unchanged)?;
        assert_eq!(sel.primary_kb.as_deref(), Some("alpha"));
        assert_eq!(svc.get_primary_persisted()?.as_deref(), Some("alpha"));

        // Same rule at session scope, where a wrong promotion is worse: the
        // machine now points at alpha, so an invented session pointer would be
        // indistinguishable from an inherited one.
        let session_file = svc.primary_session_path("s1");
        std::fs::create_dir_all(session_file.parent().unwrap())?;
        std::fs::write(&session_file, b"ghost")?;
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);

        let sel = svc.set_selection(
            Some("s1"),
            Some(&["gamma".to_string()]),
            PrimaryUpdate::Unchanged,
        )?;
        assert_eq!(sel.primary_kb, None, "a session must not invent one either");
        assert_eq!(svc.get_primary_for_session("s1")?, None);
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);
        Ok(())
    }

    /// A `KbSelection` is a claim — its `primary_kb` is a member of its own
    /// `kb_ids` — and `selection()` composed it from three separately-unlocked
    /// reads, so a writer landing between them made the claim false. Genuinely
    /// concurrent on purpose: this interleaving does not exist on one thread,
    /// so no single-threaded test can catch it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn selection_is_a_coherent_snapshot_under_a_concurrent_writer() -> anyhow::Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};

        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("alpha", "Alpha", None)?;
        svc.create_base("beta", "Beta", None)?;

        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let svc = svc.clone();
            let stop = stop.clone();
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                while !stop.load(Ordering::Relaxed) {
                    // beta in the set and pinned …
                    svc.set_selection(Some("s1"), Some(&[]), PrimaryUpdate::Set("beta"))?;
                    // … then beta out of the set entirely, which repairs the
                    // pointer onto alpha. The reader must never see a snapshot
                    // that straddles the two.
                    svc.set_selection(
                        Some("s1"),
                        Some(&["beta".to_string()]),
                        PrimaryUpdate::Unchanged,
                    )?;
                }
                Ok(())
            })
        };

        let reader = {
            let svc = svc.clone();
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                for _ in 0..3000 {
                    let sel = svc.selection(Some("s1"))?;
                    if let Some(primary) = &sel.primary_kb {
                        anyhow::ensure!(
                            sel.kb_ids.contains(primary),
                            "incoherent snapshot: primary {primary} is not in kb_ids {:?}",
                            sel.kb_ids
                        );
                    }
                }
                Ok(())
            })
        };

        let observed = reader.await?;
        stop.store(true, Ordering::Relaxed);
        writer.await??;
        observed?;
        Ok(())
    }

    /// All-or-nothing, or the invariant does not hold. The set was written
    /// before the requested primary was validated, so "hide the base I am
    /// pinned to, and pin one that does not exist" persisted the hide and
    /// *then* returned an error — leaving the stored pointer sitting outside
    /// the resulting set, which is precisely the state the merged model exists
    /// to forbid.
    #[test]
    fn a_rejected_set_selection_persists_nothing() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        for id in ["alpha", "beta"] {
            svc.create_base(id, id, None)?;
        }
        svc.set_selection(Some("s1"), Some(&[]), PrimaryUpdate::Set("beta"))?;
        let before = svc.selection(Some("s1"))?;
        assert_eq!(before.primary_kb.as_deref(), Some("beta"));

        // Hide the pinned base *and* ask for a primary that does not exist.
        let err = svc
            .set_selection(
                Some("s1"),
                Some(&["beta".to_string()]),
                PrimaryUpdate::Set("ghost"),
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("ghost"), "got: {err}");

        assert_eq!(
            svc.selection(Some("s1"))?,
            before,
            "a rejected request must not persist the half of it that ran first"
        );
        assert_eq!(
            svc.get_primary_for_session("s1")?.as_deref(),
            Some("beta"),
            "the stored pointer must still be a member of the stored set"
        );

        // A malformed id in the set half is rejected the same way.
        let err = svc
            .set_selection(
                Some("s1"),
                Some(&["NOT VALID".to_string()]),
                PrimaryUpdate::Unchanged,
            )
            .unwrap_err()
            .to_string();
        assert!(!err.is_empty());
        assert_eq!(svc.selection(Some("s1"))?, before);

        // Same at machine scope, where the vocabulary differs but the rule does not.
        svc.set_selection(None, Some(&[]), PrimaryUpdate::Set("alpha"))?;
        let before = svc.selection(None)?;
        assert!(svc
            .set_selection(
                None,
                Some(&["alpha".to_string()]),
                PrimaryUpdate::Set("ghost")
            )
            .is_err());
        assert_eq!(svc.selection(None)?, before);
        Ok(())
    }

    /// Three states, not two. "This chat has no primary" and "this chat never
    /// said" look the same to an `Option<String>` reader, so clearing a
    /// session's primary used to delete its file — the encoding of "no
    /// opinion" — and the machine-wide default walked straight back in, silently
    /// re-arming the KB-less write the user had just disarmed.
    #[test]
    fn a_session_can_explicitly_have_no_primary_while_the_machine_has_one() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("alpha", "Alpha", None)?;
        svc.create_base("beta", "Beta", None)?;
        svc.set_primary_persisted(Some("alpha"))?;

        // A chat that never chose one inherits the machine default.
        assert_eq!(
            svc.primary_for_session(Some("s1"))?.as_deref(),
            Some("alpha")
        );

        // Clearing is an override, not a reset-to-inherit.
        let sel = svc.set_selection(Some("s1"), None, PrimaryUpdate::Clear)?;
        assert_eq!(sel.primary_kb, None);
        assert_eq!(
            svc.primary_for_session(Some("s1"))?,
            None,
            "a cleared session must stay cleared, not re-inherit the machine primary"
        );

        // The machine pointer is untouched and other chats still follow it.
        assert_eq!(svc.get_primary_persisted()?.as_deref(), Some("alpha"));
        assert_eq!(
            svc.primary_for_session(Some("s2"))?.as_deref(),
            Some("alpha")
        );

        // The override is durable: it survives a fresh service over the same
        // root, i.e. it is genuinely on disk and not an in-memory artefact.
        let reopened = KnowledgeService::new(tmp.path().to_path_buf());
        assert_eq!(reopened.primary_for_session(Some("s1"))?, None);
        assert_eq!(reopened.selection(Some("s1"))?.primary_kb, None);
        assert_eq!(reopened.get_primary_for_session("s1")?, None);

        // Set-only edits must not resurrect it either.
        let sel = svc.set_selection(Some("s1"), Some(&[]), PrimaryUpdate::Unchanged)?;
        assert_eq!(sel.primary_kb, None);

        // Dropping the override is a separate, explicit gesture.
        let sel = svc.set_selection(Some("s1"), None, PrimaryUpdate::Inherit)?;
        assert_eq!(sel.primary_kb.as_deref(), Some("alpha"));
        svc.set_primary_for_session("s1", None)?;
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);
        svc.clear_primary_override_for_session("s1")?;
        assert_eq!(
            svc.primary_for_session(Some("s1"))?.as_deref(),
            Some("alpha")
        );
        Ok(())
    }

    /// Deleting the base a chat had pinned must leave that chat with *no*
    /// primary. The delete rewriter removed the session's file, which is
    /// "never chose" — so the chat silently adopted the machine-wide default
    /// it had never selected, and the next KB-less write landed there.
    #[test]
    fn deleting_a_pinned_base_does_not_hand_the_session_the_machine_primary() -> anyhow::Result<()>
    {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("alpha", "Alpha", None)?;
        svc.create_base("beta", "Beta", None)?;
        svc.set_primary_persisted(Some("alpha"))?;
        svc.set_primary_for_session("s1", Some("beta"))?;

        svc.delete_base("beta")?;

        assert_eq!(
            svc.primary_for_session(Some("s1"))?,
            None,
            "the chat pinned the deleted base; it must not inherit 'alpha' now"
        );
        assert_eq!(
            svc.primary_for_session(Some("s2"))?.as_deref(),
            Some("alpha"),
            "a chat that never chose one still follows the machine default"
        );
        Ok(())
    }

    /// …and the chat it leaves that way must have a way back. The blank
    /// override is durable by design, which made it a **one-way door**: no set
    /// edit lifts it, `Clear` reinstates it, and `Set` is only available if the
    /// user wants to pin something specific. A chat that lost its pinned base
    /// to a delete could therefore never follow the machine-wide default again.
    /// `PrimaryUpdate::Inherit` is the escape hatch, and it must leave the
    /// chat genuinely *following* the pointer rather than having copied it.
    #[test]
    fn inherit_lifts_the_no_primary_override_a_delete_installed() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        for id in ["alpha", "beta", "gamma"] {
            svc.create_base(id, id, None)?;
        }
        svc.set_primary_persisted(Some("alpha"))?;
        svc.set_primary_for_session("s1", Some("gamma"))?;
        svc.delete_base("gamma")?;
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);

        // Every other gesture leaves the override standing.
        svc.set_selection(Some("s1"), Some(&[]), PrimaryUpdate::Unchanged)?;
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);
        svc.set_selection(Some("s1"), None, PrimaryUpdate::Clear)?;
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);

        let sel = svc.set_selection(Some("s1"), None, PrimaryUpdate::Inherit)?;
        assert_eq!(sel.primary_kb.as_deref(), Some("alpha"));

        // Following, not a one-time copy: the chat tracks later machine moves.
        svc.set_primary_persisted(Some("beta"))?;
        assert_eq!(
            svc.primary_for_session(Some("s1"))?.as_deref(),
            Some("beta")
        );
        assert_eq!(
            svc.get_primary_for_session("s1")?,
            None,
            "inheriting means holding no pointer of its own"
        );

        // At machine scope there is nothing above to inherit, so the two
        // spellings coincide — and neither leaves debris behind.
        let sel = svc.set_selection(None, None, PrimaryUpdate::Inherit)?;
        assert_eq!(sel.primary_kb, None);
        assert!(!crate::knowledge::paths::primary_kb_path(svc.root()).exists());
        Ok(())
    }

    #[test]
    fn hidden_kbs_track_rename_and_delete() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());

        svc.create_base("kb-a", "KB A", None)?;
        svc.set_hidden_persisted(&["kb-a".to_string()])?;
        svc.set_hidden_for_session("session-a", &["kb-a".to_string()])?;

        let renamed = svc.update_base("kb-a", Some("Renamed KB"), None)?;
        assert_eq!(renamed.id, "renamed-kb");
        assert_eq!(svc.get_hidden_persisted()?, vec!["renamed-kb".to_string()]);
        assert_eq!(
            svc.get_hidden_for_session("session-a")?,
            vec!["renamed-kb".to_string()]
        );

        svc.delete_base("renamed-kb")?;
        assert!(svc.get_hidden_persisted()?.is_empty());
        assert!(svc.get_hidden_for_session("session-a")?.is_empty());

        Ok(())
    }

    // -----------------------------------------------------------------------
    // migrate_schema_if_needed tests
    // -----------------------------------------------------------------------

    /// Legacy schema fingerprint: minimal pre-Plan-5-Task-2 schema (no
    /// "Cross-reference rules" section).
    const LEGACY_SCHEMA: &str =
        "# Knowledge Base: Maintenance Schema\n\n## Layout\n\n- wiki/sources/...\n";

    #[test]
    fn migrate_schema_appends_cross_reference_rules_when_missing() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        // Overwrite with the legacy schema and commit so we have a clean
        // baseline to migrate from.
        std::fs::write(kb.join("schema.md"), LEGACY_SCHEMA).unwrap();
        let repo = GitRepo::open(&kb).unwrap();
        repo.commit_all(ChangeKind::Manual, "seed legacy schema", None)
            .unwrap();
        let before = repo.log(10).unwrap().len();

        let migrated = svc.migrate_schema_if_needed("k").unwrap();
        assert!(migrated, "first call should migrate");

        let new_schema = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert!(new_schema.contains("Cross-reference rules"));
        // Original content is preserved.
        assert!(new_schema.contains("wiki/sources/..."));

        let after = repo.log(10).unwrap().len();
        assert_eq!(after, before + 1, "exactly one migration commit added");
        assert!(repo.log(1).unwrap()[0].summary.contains("migrate schema"));
    }

    #[test]
    fn migrate_schema_is_noop_when_already_present() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");

        // create_base already writes the current DEFAULT_SCHEMA, which
        // contains the cross-reference rules section.
        let original = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert!(original.contains("Cross-reference rules"));
        let repo = GitRepo::open(&kb).unwrap();
        let before = repo.log(10).unwrap().len();

        let migrated = svc.migrate_schema_if_needed("k").unwrap();
        assert!(!migrated, "already-migrated KB should be a no-op");

        let after_schema = std::fs::read_to_string(kb.join("schema.md")).unwrap();
        assert_eq!(after_schema, original, "schema bytes unchanged");
        let after = repo.log(10).unwrap().len();
        assert_eq!(after, before, "no new commit");
    }

    #[test]
    fn migrate_schema_is_idempotent_across_calls() {
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        let kb = svc.root().join("k");
        std::fs::write(kb.join("schema.md"), LEGACY_SCHEMA).unwrap();
        let repo = GitRepo::open(&kb).unwrap();
        repo.commit_all(ChangeKind::Manual, "seed legacy schema", None)
            .unwrap();

        // First call migrates.
        assert!(svc.migrate_schema_if_needed("k").unwrap());
        // Second call is a no-op.
        assert!(!svc.migrate_schema_if_needed("k").unwrap());
        // Third call too.
        assert!(!svc.migrate_schema_if_needed("k").unwrap());
    }

    #[tokio::test]
    async fn registering_a_tier_from_inside_the_root_lock_does_not_deadlock() {
        // The whole of decision (5b), as a test that TIMES OUT rather than fails
        // if the `_unlocked` convention is broken — a deadlock does not assert,
        // it waits. `create_base` holds `lock_root()`; a `tier::raise` that
        // acquires it again blocks forever and the daemon stops answering on the
        // very first knowledge call, while every tier.rs unit test still passes
        // because they call the store on a bare root no service is holding.
        let d = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(d.path().to_path_buf());
        let done = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                svc.create_base("k", "K", None)?; // registers, inside the lock
                                                  // the wrapper: takes the lock itself, once, for both axes
                svc.raise_tier_and_affiliation(
                    "k",
                    true,
                    &crate::knowledge::affiliation::CallerAffiliation::Institution(
                        "ucsf".to_string(),
                    ),
                )?;
                svc.delete_base("k") // forgets, inside the lock
            }),
        )
        .await
        .expect("create_base / raise_tier / delete_base deadlocked on the root lock");
        done.unwrap().unwrap();
    }

    #[test]
    fn a_base_created_by_any_surface_is_registered_public_rather_than_unknown() {
        // Decision (5a). Without the registration, decision (3) reads a freshly
        // created base as PRIVATE and Task 10C locks the user out of a base they
        // just made from the CLI or the Knowledge view.
        let (_dir, svc) = svc();
        svc.create_base("k", "K", None).unwrap();
        assert!(!crate::knowledge::tier::is_private(svc.root(), "k"));
    }
}
