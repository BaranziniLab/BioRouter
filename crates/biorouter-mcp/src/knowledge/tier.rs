//! The knowledge-base privacy tier (issue #56, design §9.3 B4).
//!
//! A knowledge base takes the tier of the most sensitive session that has
//! ingested into it. This module owns the store, the monotone raise and the
//! refusal; Task 10B calls `raise` from the four choke points and Task 10C
//! calls [`assert_reachable`] from the same four.
//!
//! A `bool` rather than an enum because `biorouter-mcp` cannot depend on
//! `biorouter`, where `ProviderTier` lives — see the task's decision (1).
//! `caller_is_private == true` is exactly `floor(Private) == Private`.
//!
//! ## Locking
//!
//! Every mutator here is `_unlocked` and takes NO lock. The knowledge root has
//! exactly one lock, `KnowledgeService::lock_root`, and both it and
//! `FileLockGuard` are private to the `service` module — a free function cannot
//! reach them, and `create_base`/`import_brkb`/`delete_base` are already inside
//! it when they call these. A second `FileLockGuard::acquire` in the same
//! process blocks forever (it opens a fresh fd and `flock`s exclusively).
//! Callers OUTSIDE the service use `KnowledgeService::raise_tier` /
//! `forget_tier`, which take the lock and delegate here. This is the tree's own
//! `_unlocked` convention — twenty helpers in `service.rs` already follow it.
//!
//! ## Known residuals
//!
//! ⚠ An earlier version of this block said `lock_root()` was in-process, that
//! two Biorouter processes could therefore lose a raise, and that closing it
//! "needs an OS advisory lock the tree does not have anywhere". All three are
//! false: `FileLockGuard::acquire` calls `file.lock_exclusive()` through
//! `fs2::FileExt` (`service.rs`), which IS flock/`LockFileEx` — cross-process,
//! and the same mechanism the paragraph above correctly relies on when it
//! explains why a second acquire in one process blocks. It is corrected rather
//! than deleted because inviting someone to "fix" a race the tree already closes
//! is worse than saying nothing, and because it obscured the two that are real:
//!
//! 1. **Readers take no lock, by design.** `is_private` and `has_entry_unlocked`
//!    never `flock`; they rely on the store being replaced only by `rename`, so
//!    a read sees the old file or the new one and never a torn one. What that
//!    does not give is freshness: a permit decided microseconds before a raise
//!    lands is still acted on. That is check-then-use, it is the same window
//!    Task 10A's own coverage note records for `primary_kb_for_context`, and
//!    closing it would mean holding the root lock across every knowledge read.
//! 2. **The lock is advisory and file-scoped.** Anything that edits `.kb-tiers`
//!    without going through `KnowledgeService` — a text editor, another tool —
//!    bypasses it entirely. The task accepts that: the file is machine-local and
//!    user-writable, and the threat model is "a public model reads private
//!    notes", not "a local attacker with a shell".

use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

const SCHEMA: u32 = 1;

const PUBLIC: &str = "public";
const PRIVATE: &str = "private";

/// The meta key the daemon writes the caller's capability into. Defined here,
/// not in `server.rs`, because `agent_drafter` (CP4) reads the same key and two
/// spellings of it is exactly how a barrier silently stops working.
pub const CAPABILITY_TIER_META_KEY: &str = "biorouter-capability-tier";

/// Names no base, no page and no snippet. Constant, so a model that retries sees
/// the same string and stops rather than looping.
pub const KB_PRIVATE_REFUSAL: &str = "\
This knowledge base is private: a session running an institutional or self-hosted model has \
ingested into it, so only a private model may read or write it. This session is running on a \
public model. Ask the user to switch this chat to a private model — Settings > Models, or the \
model chip in the composer — and try again. Do not retry with a different knowledge base id, \
through an export, or through a raw-source search; the boundary is the same everywhere.";

#[derive(serde::Serialize, serde::Deserialize)]
struct Store {
    schema: u32,
    /// kb id -> "public" | "private". An id ABSENT from a store that exists,
    /// for a directory that DOES exist, is unknown provenance and reads
    /// PRIVATE; the whole file being absent means the migration has not run.
    bases: BTreeMap<String, String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            bases: BTreeMap::new(),
        }
    }
}

/// What a read of `.kb-tiers` found. `Missing` and `Unreadable` are deliberately
/// distinct: the first means the migration has not run (⇒ everything public,
/// AR-2's accepted fail-open direction), the second means the answer is unknown
/// (⇒ fail closed).
enum StoreState {
    Missing,
    Parsed(Store),
    Unreadable,
}

fn load(root: &Path) -> StoreState {
    let p = super::paths::kb_tiers_path(root);
    let raw = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StoreState::Missing,
        Err(e) => {
            tracing::error!(
                "knowledge: cannot read {} ({e}); every base reads PRIVATE until it is readable",
                p.display()
            );
            return StoreState::Unreadable;
        }
    };
    match serde_json::from_str::<Store>(&raw) {
        Ok(store) => StoreState::Parsed(store),
        Err(e) => {
            tracing::error!(
                "knowledge: {} is not parseable ({e}); every base reads PRIVATE until it is",
                p.display()
            );
            StoreState::Unreadable
        }
    }
}

/// The store as it must be before a write.
///
/// A missing file is an empty store. An **unreadable** one is refused: silently
/// replacing a store we cannot parse would erase every ratchet in it, which is
/// the one direction this module exists to prevent.
fn load_for_write(root: &Path) -> Result<Store> {
    match load(root) {
        StoreState::Missing => Ok(Store::default()),
        StoreState::Parsed(store) => Ok(store),
        StoreState::Unreadable => anyhow::bail!(
            "the knowledge-base tier store is unreadable, so it cannot be updated without \
             erasing the classifications already in it. Repair or remove {}",
            super::paths::kb_tiers_path(root).display()
        ),
    }
}

/// `manifest::save`'s idiom: write a sibling tmp file and `rename` over the
/// target, so a reader sees the old file or the new one, never a torn one.
fn save(root: &Path, store: &Store) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let path = super::paths::kb_tiers_path(root);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(store)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

fn tier_word(caller_is_private: bool) -> &'static str {
    if caller_is_private {
        PRIVATE
    } else {
        PUBLIC
    }
}

/// The value the daemon writes under [`CAPABILITY_TIER_META_KEY`].
///
/// The KEY was already shared so the two sides cannot drift; the VALUE was not —
/// `mcp_client.rs` spelled "private"/"public" itself and [`caller_is_private`]
/// compared against this module's own consts, and a test on each side asserting
/// its own literal would have caught neither half of a drift. One spelling, one
/// function, both sides.
pub fn capability_meta_value(caller_is_private: bool) -> &'static str {
    tier_word(caller_is_private)
}

/// The caller's capability, PUBLIC unless the meta says private.
///
/// Absent means one of: an older daemon, a non-built-in transport, or a direct
/// unit-test construction. All three are "unknown", and unknown must be the
/// restrictive answer for the reads Task 10C gates.
pub fn caller_is_private(meta: &rmcp::model::Meta) -> bool {
    meta.0
        .get(CAPABILITY_TIER_META_KEY)
        .and_then(|v| v.as_str())
        == Some(PRIVATE)
}

/// One-time migration. Every base that exists when this first runs becomes
/// PUBLIC (fail-open, DR-10 and AR-2). Guarded by the file's absence: re-running
/// it on every startup would re-add a base whose entry a later `forget` removed,
/// and would race the ratchet.
pub fn ensure_migrated_unlocked(root: &Path) -> Result<()> {
    if super::paths::kb_tiers_path(root).exists() {
        return Ok(());
    }
    let mut store = Store::default();
    if root.exists() {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // The sidecar directories (`.hidden-kb-sessions`, …) all start with
            // a dot and so fail id validation; nothing else in the root is a
            // knowledge base.
            if super::paths::validate_kb_id(&name).is_err() {
                continue;
            }
            store.bases.insert(name, PUBLIC.to_string());
        }
    }
    save(root, &store)
}

/// PRIVATE unless the store says otherwise — with one exception that is the
/// point of decision (3): a kb id with **no entry and no directory under
/// `root`** is not private, because there is nothing there to leak and refusing
/// would ban a public session from creating or importing a base.
///
/// An explicit entry always wins, directory or not: `raise` registers ids that
/// have not been created yet (an import lands on one), and a registration that
/// a missing directory could override would not be a ratchet.
///
/// Otherwise fail-closed on: no entry for a directory that exists, an
/// unparseable file, an unreadable file. Each of the last two logs at `error!`.
/// Lock-free: the store is only ever replaced by `rename`, so a reader sees the
/// old file or the new one, never a torn one.
pub fn is_private(root: &Path, kb_id: &str) -> bool {
    match load(root) {
        // The migration has not run, so every base is public (AR-2).
        StoreState::Missing => false,
        StoreState::Parsed(store) => match store.bases.get(kb_id) {
            // Only the exact word `public` permits. Anything else parseable but
            // unrecognised — a hand-edit, a future schema's word read by an
            // older binary — is unknown, and unknown fails in the same direction
            // as an unparseable file.
            Some(tier) => tier != PUBLIC,
            // Unknown provenance if the base is really there; nothing to leak
            // if it is not.
            None => super::paths::kb_root(root, kb_id).exists(),
        },
        StoreState::Unreadable => super::paths::kb_root(root, kb_id).exists(),
    }
}

/// The single refusal for CP1..CP4. `Ok(())` permits.
pub fn assert_reachable(root: &Path, kb_id: &str, caller_is_private: bool) -> Result<()> {
    if caller_is_private || !is_private(root, kb_id) {
        return Ok(());
    }
    anyhow::bail!(KB_PRIVATE_REFUSAL)
}

/// Monotone. Registers `kb_id` at the caller's tier if absent, raises it to
/// private if the caller is private, and can never lower it.
pub fn raise_unlocked(root: &Path, kb_id: &str, caller_is_private: bool) -> Result<()> {
    let mut store = load_for_write(root)?;
    match store.bases.get(kb_id) {
        // Anything that is not the exact word `public` already reads private
        // (see `is_private`), so a raise is a no-op and must not overwrite it —
        // replacing an unrecognised word with `public` would be a lowering.
        Some(current) if current != PUBLIC => return Ok(()),
        Some(_) if !caller_is_private => return Ok(()),
        _ => {}
    }
    store
        .bases
        .insert(kb_id.to_string(), tier_word(caller_is_private).to_string());
    save(root, &store)
}

/// Register `kb_id` as PUBLIC **only if it has no entry**. Called by
/// `create_base` and `import_brkb` (decision 5a): a base with no entry reads
/// private by decision (3), so an unregistered base would lock its own creator
/// out. Never lowers.
pub fn register_public_if_absent_unlocked(root: &Path, kb_id: &str) -> Result<()> {
    let mut store = load_for_write(root)?;
    if store.bases.contains_key(kb_id) {
        return Ok(());
    }
    store.bases.insert(kb_id.to_string(), PUBLIC.to_string());
    save(root, &store)
}

/// Whether the store already carries an entry for `kb_id`, whether or not the
/// base exists on disk.
///
/// `brkb::import`'s collision loop asks this alongside its directory test: an
/// entry can exist without a directory (`raise_unlocked` registers ids that have
/// not been created — decision (3)'s third direction), and an import that landed
/// on one would take its classification from a base that never existed.
pub fn has_entry_unlocked(root: &Path, kb_id: &str) -> bool {
    match load(root) {
        StoreState::Missing => false,
        StoreState::Parsed(store) => store.bases.contains_key(kb_id),
        // NOT `true`: this answer drives a `while` loop, and "every id is taken"
        // spins forever. An unreadable store fails closed at the only place it
        // matters anyway — `is_private` reads any existing directory as private
        // when it cannot parse the file, and the import has just created one.
        StoreState::Unreadable => false,
    }
}

/// Drop the entry when the base is deleted, so a later base reusing the id is
/// classified by its own creator rather than by a base that no longer exists.
pub fn forget_unlocked(root: &Path, kb_id: &str) -> Result<()> {
    let mut store = load_for_write(root)?;
    if store.bases.remove(kb_id).is_none() {
        return Ok(());
    }
    save(root, &store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Returns the `TempDir` alongside the root: dropping it deletes the
    /// directory, so the plan's `let root = tempdir_with_bases(..)` would unlink
    /// the tree before the first assertion ran.
    fn tempdir_with_bases(ids: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().to_path_buf();
        for id in ids {
            std::fs::create_dir_all(root.join(id).join("knowledge")).unwrap();
        }
        (d, root)
    }

    fn tiers_file_exists(root: &Path) -> bool {
        crate::knowledge::paths::kb_tiers_path(root).exists()
    }

    fn seed_page(root: &Path, kb_id: &str, rel: &str, body: &str) {
        let p = root.join(kb_id).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn page_body(root: &Path, kb_id: &str, rel: &str) -> String {
        std::fs::read_to_string(root.join(kb_id).join(rel)).unwrap()
    }

    fn export_brkb_with_provenance(root: &Path, kb_id: &str) -> Vec<u8> {
        crate::knowledge::service::KnowledgeService::new(root.to_path_buf())
            .export_brkb(kb_id)
            .unwrap()
    }

    fn import_brkb_as(root: &Path, bytes: &[u8], importer_is_private: bool) -> String {
        crate::knowledge::service::KnowledgeService::new(root.to_path_buf())
            .import_brkb(bytes, importer_is_private)
            .unwrap()
    }

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..a.len())
            .map(|i| a.by_index(i).unwrap().name().to_string())
            .collect()
    }

    fn archive(id: &str, marker: Option<bool>) -> Vec<u8> {
        archive_inner(
            id,
            marker.map(|private| {
                serde_json::json!({
                    "schema": 1,
                    "tier": if private { "private" } else { "public" },
                })
                .to_string()
            }),
        )
    }

    fn archive_with_raw_marker(id: &str, raw: &str) -> Vec<u8> {
        archive_inner(id, Some(raw.to_string()))
    }

    fn archive_inner(id: &str, marker: Option<String>) -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(format!("{id}/knowledge/x.md"), opts)
                .unwrap();
            zip.write_all(b"body").unwrap();
            if let Some(m) = marker {
                zip.start_file(format!("{id}/.brkb-provenance"), opts)
                    .unwrap();
                zip.write_all(m.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn an_unmigrated_root_migrates_every_existing_base_to_public_exactly_once() {
        // AR-2, made executable. The tree keeps no record of which session wrote
        // which page — the git author is "Biorouter", not a session id — so there
        // is nothing to reason from and the migration fails OPEN, like the session
        // backfill (DR-10). It must run ONCE: a startup-repeating "add any base
        // missing from the file as public" would un-ratchet a base the day after a
        // private session raised it, which is Task 38's bug in a different store.
        let (_d, root) = tempdir_with_bases(&["default", "omop"]);
        assert!(!tiers_file_exists(&root));

        ensure_migrated_unlocked(&root).unwrap();
        assert!(!is_private(&root, "default"));
        assert!(!is_private(&root, "omop"));

        raise_unlocked(&root, "omop", /* caller_is_private */ true).unwrap();
        ensure_migrated_unlocked(&root).unwrap(); // second launch
        assert!(
            is_private(&root, "omop"),
            "the migration re-ran and lowered a tier"
        );
    }

    #[test]
    fn a_base_that_never_went_through_create_or_import_reads_private() {
        // Fail-closed, and it is the difference between "known public" and
        // "unknown". A store that listed only the private ids could not tell them
        // apart, which is why the file is a map and not a list like `.hidden-kbs`.
        let (_d, root) = tempdir_with_bases(&["default"]);
        ensure_migrated_unlocked(&root).unwrap();
        std::fs::create_dir_all(root.join("dropped-in-by-hand")).unwrap();
        assert!(is_private(&root, "dropped-in-by-hand"));
        assert!(!is_private(&root, "default"));
    }

    #[test]
    fn a_base_that_does_not_exist_on_disk_is_reachable_by_anyone() {
        // Decision (3), third direction — and the bug the sixteen-site enumeration
        // encoded: `kb_create_base` and `kb_import` name a base that does not exist
        // yet, so a barrier that reads "no entry ⇒ private" bans a public session
        // from ever creating or importing one. There is no content to leak from a
        // directory that is not there.
        let (_d, root) = tempdir_with_bases(&["default"]);
        ensure_migrated_unlocked(&root).unwrap();
        assert!(!is_private(&root, "not-created-yet"));
        assert_reachable(&root, "not-created-yet", /* caller_is_private */ false).unwrap();
    }

    #[test]
    fn raise_is_monotone_and_registers_an_absent_base_at_the_callers_tier() {
        let (_d, root) = tempdir_with_bases(&[]);
        ensure_migrated_unlocked(&root).unwrap();

        raise_unlocked(&root, "fresh", false).unwrap(); // created from a public chat
        assert!(!is_private(&root, "fresh"));
        raise_unlocked(&root, "fresh", true).unwrap(); // a private chat writes to it
        assert!(is_private(&root, "fresh"));
        raise_unlocked(&root, "fresh", false).unwrap(); // and a public chat writes again
        assert!(
            is_private(&root, "fresh"),
            "a public write lowered the tier"
        );

        raise_unlocked(&root, "born-private", true).unwrap();
        assert!(is_private(&root, "born-private"));
    }

    #[test]
    fn register_public_never_lowers_an_already_private_base() {
        // `create_base` registers unconditionally (decision 5a). If that registration
        // could overwrite, then re-creating a deleted id — or any future caller that
        // registers twice — would launder a private base to public.
        let (_d, root) = tempdir_with_bases(&["omop"]);
        ensure_migrated_unlocked(&root).unwrap();
        raise_unlocked(&root, "omop", true).unwrap();
        register_public_if_absent_unlocked(&root, "omop").unwrap();
        assert!(
            is_private(&root, "omop"),
            "registration lowered a ratcheted base"
        );
    }

    #[test]
    fn deleting_a_base_forgets_its_tier_so_the_id_can_be_reused() {
        // Otherwise `kb_create_base("omop")` from a public chat, after a private
        // `omop` was deleted, silently inherits Private and the user cannot see why.
        let (_d, root) = tempdir_with_bases(&["omop"]);
        ensure_migrated_unlocked(&root).unwrap();
        raise_unlocked(&root, "omop", true).unwrap();
        forget_unlocked(&root, "omop").unwrap();
        raise_unlocked(&root, "omop", false).unwrap();
        assert!(!is_private(&root, "omop"));
    }

    #[test]
    fn the_store_is_written_atomically_and_never_leaves_a_tmp_file() {
        // manifest.rs:17-24's idiom. A torn write here reads as "no entry", which
        // fails CLOSED and locks the user out of their own knowledge base.
        let (_d, root) = tempdir_with_bases(&["default"]);
        ensure_migrated_unlocked(&root).unwrap();

        // ⚠ The final-state half below — the file is there, no `*.tmp` beside it
        // — is satisfied just as well by a plain `fs::write` straight to the
        // target, which is the very implementation this test exists to reject.
        // The inode is what tells the two apart: `rename` puts a DIFFERENT file
        // at the path, while `fs::write` truncates the one already there and
        // keeps its inode. That is the atomicity, observed rather than described.
        #[cfg(unix)]
        let before = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(crate::knowledge::paths::kb_tiers_path(&root))
                .unwrap()
                .ino()
        };

        raise_unlocked(&root, "default", true).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let after = std::fs::metadata(crate::knowledge::paths::kb_tiers_path(&root))
                .unwrap()
                .ino();
            assert_ne!(
                before, after,
                "the store was written in place: a reader can see a half-written file"
            );
        }

        let names: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(names.iter().any(|n| n == ".kb-tiers"));
        assert!(!names.iter().any(|n| n.ends_with(".tmp")));
    }

    #[test]
    fn a_private_export_cannot_be_laundered_by_importing_it_into_a_public_chat() {
        // The two-call bypass, end to end and in the ONE direction the previous
        // tests never ran: export PRIVATE, import PUBLIC. Before decision (2)'s
        // marker the imported base takes the importer's tier and every page of a
        // private base is readable by a public model, with no gate crossed —
        // `kb_export` is permitted (the base is the private caller's own) and
        // `kb_import` is permitted (the id it creates does not exist yet).
        let (_d, root) = tempdir_with_bases(&["omop"]);
        ensure_migrated_unlocked(&root).unwrap();
        raise_unlocked(&root, "omop", true).unwrap();
        seed_page(&root, "omop", "knowledge/x.md", "SENTINEL-COHORT-N-412");

        let bytes = export_brkb_with_provenance(&root, "omop"); // private base
        let new_id = import_brkb_as(&root, &bytes, /* importer_is_private */ false);

        assert_ne!(new_id, "omop", "the collision loop must land on a fresh id");
        assert!(
            is_private(&root, &new_id),
            "a private base was laundered into a public one by export/import"
        );
        // And the content really did arrive — otherwise the assertion above is
        // satisfied by an import that failed.
        assert!(page_body(&root, &new_id, "knowledge/x.md").contains("SENTINEL-COHORT-N-412"));
    }

    #[test]
    fn the_provenance_marker_can_only_raise_and_a_foreign_archive_is_unaffected() {
        // Decision (2)'s whole safety argument, as three rows. The marker is
        // attacker-supplied by construction — it rides inside the zip — and is safe
        // ONLY because it is read as a floor.
        let (_d, root) = tempdir_with_bases(&[]);
        ensure_migrated_unlocked(&root).unwrap();

        // (a) marker private + importer public  -> private   (the laundering case)
        assert!(is_private(
            &root,
            &import_brkb_as(&root, &archive("a", Some(true)), false)
        ));
        // (b) marker PUBLIC + importer private  -> private   (a hostile archive
        //     claiming "public" cannot lower the importing session's own tier)
        assert!(is_private(
            &root,
            &import_brkb_as(&root, &archive("b", Some(false)), true)
        ));
        // (c) NO marker (a foreign .brkb, or one written before this task) +
        //     importer public -> public: unknown means the importer's tier, which
        //     is today's behaviour and must not regress into "everything imported
        //     is private", a state with no declassification path (AR-1).
        assert!(!is_private(
            &root,
            &import_brkb_as(&root, &archive("c", None), false)
        ));
        // (d) a malformed marker is read as absent, not as private — same reason.
        assert!(!is_private(
            &root,
            &import_brkb_as(&root, &archive_with_raw_marker("d", "yes"), false)
        ));
    }

    #[test]
    fn the_marker_rides_in_the_archive_and_never_lands_on_disk() {
        // It is written straight into the ZipWriter after `walk` (brkb.rs:16-17),
        // so the KB's git tree does not gain an untracked file — and it goes INSIDE
        // the single top-level directory, because `import` (:70-87) bails unless
        // there is exactly one and a sibling entry would break every archive.
        let (_d, root) = tempdir_with_bases(&["omop"]);
        ensure_migrated_unlocked(&root).unwrap();
        raise_unlocked(&root, "omop", true).unwrap();
        let bytes = export_brkb_with_provenance(&root, "omop");
        assert!(zip_names(&bytes).contains(&"omop/.brkb-provenance".to_string()));
        assert!(
            !root.join("omop/.brkb-provenance").exists(),
            "the marker was written to disk"
        );
        // …and it is not extracted back out into the imported base either.
        let new_id = import_brkb_as(&root, &bytes, false);
        assert!(!root.join(&new_id).join(".brkb-provenance").exists());
    }

    #[test]
    fn an_unrecognised_tier_word_reads_private_and_no_public_write_replaces_it() {
        // The file is machine-local and user-writable, which the task accepts —
        // but "parseable JSON carrying a word we do not recognise" must fail in
        // the same direction as "unparseable file" (PRIVATE), not the opposite
        // one. `tier == PRIVATE` read a typo as public; `tier != PUBLIC` does
        // not, and only the exact word `public` can ever permit.
        let (_d, root) = tempdir_with_bases(&["omop"]);
        ensure_migrated_unlocked(&root).unwrap();
        std::fs::write(
            crate::knowledge::paths::kb_tiers_path(&root),
            r#"{"schema":1,"bases":{"omop":"privte"}}"#,
        )
        .unwrap();

        assert!(is_private(&root, "omop"), "a typo'd tier word read PUBLIC");
        // …and the ratchet treats it as already at least as restrictive as
        // private, so a public write cannot overwrite it into a permit.
        raise_unlocked(&root, "omop", false).unwrap();
        assert!(
            is_private(&root, "omop"),
            "a public write replaced an unrecognised word"
        );
    }

    #[test]
    fn an_import_never_lands_on_an_id_that_only_the_tier_store_still_knows() {
        // `brkb::import`'s collision loop suffixes while the DIRECTORY exists,
        // and the plan's justification for stamping a tier after the import
        // ("it always lands on a fresh id") is true for directories and false
        // for entries. `raise_unlocked` registers ids that have no directory —
        // that is decision (3)'s third direction and Task 10B will do it at the
        // choke points — so an entry can outlive, or precede, any base. Landing
        // on one would classify freshly imported content by a base that never
        // existed: fail-closed if the ghost is private, but decided by the wrong
        // thing either way.
        let (_d, root) = tempdir_with_bases(&[]);
        ensure_migrated_unlocked(&root).unwrap();
        raise_unlocked(&root, "orig", true).unwrap(); // an entry, no directory
        assert!(!root.join("orig").exists());

        let new_id = import_brkb_as(&root, &archive("orig", None), false);
        assert_ne!(new_id, "orig", "the import landed on a ghost entry's id");
        assert!(
            !is_private(&root, &new_id),
            "a fresh public import was classified by a base that never existed"
        );
        // The ghost is untouched — the import neither adopted nor cleared it.
        assert!(is_private(&root, "orig"));
    }

    #[test]
    fn the_refusal_names_no_base_and_no_page() {
        // One string serves CP1..CP4, so it is asserted once, here.
        let (_d, root) = tempdir_with_bases(&["omop"]);
        ensure_migrated_unlocked(&root).unwrap();
        raise_unlocked(&root, "omop", true).unwrap();
        let s = assert_reachable(&root, "omop", false)
            .unwrap_err()
            .to_string();
        assert!(s.contains("private model"));
        assert!(!s.contains("omop"), "the refusal named the base: {s}");
    }
}
