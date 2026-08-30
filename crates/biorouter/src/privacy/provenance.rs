//! Where an installed extension came from — issue #56, [DR-23].
//!
//! DR-23 says an extension's tier is **re-derived from the registry at read
//! time, keyed on a stable identifier, and never written onto the local config
//! entry**. Measurement said the stable identifier did not exist: a `.brxt`
//! install recorded no provenance whatsoever. `BrxtInstallModal.tsx` wrote
//! `{name, description, type, cmd, args, envs, env_keys, timeout}` into
//! `ExtensionConfig::Stdio` and `installBrxtBundle` returned only
//! `{ installDir }`, so the only join available was the accident that
//! `name_to_key` reduces the registry `id` (`cdwagent`) and the bundle
//! `manifest.name` (`CDWAgent`) to the same string. Rename the config entry and
//! the join misses — which removed **enforcement**, since Gates C, E, F1 and F2
//! all read that answer, not merely the badge.
//!
//! This module is the missing half: the install path records the registry id
//! (plus the download URL and the bundle's hash, which cost nothing to keep and
//! are the two facts an incident response would want) beside the config entry,
//! and [`classify_extension`](super::classify_extension) resolves through it.
//!
//! # Why this is not on `ExtensionConfig`
//!
//! `extension_manager.rs` already records the reason `Extension.tier` was kept
//! off that struct: `ExtensionConfig` round-trips through user-writable
//! `config.yaml`, so anything on it is agent-editable. That is the exact hole
//! DR-23 exists to close, so provenance lives in its own file.
//!
//! # Why this file needs no gated writer — for the TIER
//!
//! Because the resolver **unions** rather than replaces. `classify_extension`
//! returns Private when *either* the config name or the recorded registry id is
//! in the compiled marketplace snapshot, so a record can only ever RAISE a
//! tier. Forging one, corrupting the file, or deleting it outright cannot lower
//! the tier below what the name alone already said — which is precisely
//! DR-23's own argument that re-deriving *removes* the problem instead of
//! guarding it. Sessions have exactly one gated writer because a session's tier
//! is stored; an extension's is not, and this file stores identity, not tier.
//!
//! ⚠ **That argument is about the tier and does not extend to the AFFILIATION,
//! which Task 47 added to the same resolution.** Read this paragraph before
//! citing the one above as the reason a new consumer needs no writer gate.
//!
//! Union raises a tier because Private is the restrictive end of that lattice.
//! On the third axis the direction inverts: an
//! [`ExtensionAffiliation::Institutions`](super::affiliation::ExtensionAffiliation::Institutions)
//! set is an **allowlist**, so unioning two of them produces a *more* permissive
//! extension — reachable from more institutions' models without the
//! cross-affiliation warning DR-26 requires. An extra identity therefore raises
//! the tier and relaxes the affiliation in the same step.
//!
//! It is reachable the same way the tier's identities are: `config.yaml` is
//! agent-writable (DR-17 descoped the filesystem barrier), and an entry whose
//! `args` name another affiliated extension's recorded `install_dir` acquires
//! that extension's institutions. Where the name alone answers `{ucsf}`, the
//! unioned answer `{ucsf, stanford}` clears a Stanford-bound model's mismatch
//! silently — and DR-26 is explicit that an agent may never clear a mismatch
//! automatically.
//!
//! ⚠ **Latent, not live, and the distinction is the whole of the risk
//! assessment.** Every affiliated extension this build ships is `ucsf`, so every
//! union that can currently be formed is `{ucsf}` and no relaxation exists to
//! reach. It goes live the day a second institution enters `INSTITUTIONS` — i.e.
//! before Phase 6 is finished, not at some indefinite later date. The union is
//! what Task 47 specifies ("a base matching several institutions carries all of
//! them"), and *carrying* several institutions' data argues for the opposite
//! compatibility rule from *permitting* several institutions' models, so this
//! needs an explicit ruling rather than an implementer's guess. Recorded here
//! because whoever builds Tasks 48-51 will arrive at this header looking for the
//! reason forged records are harmless, and for the affiliation they are not.
//!
//! ⚠ **What a forged record CAN do is deny, and that is worth saying out loud.**
//! Raise-only means no disclosure, not no harm: a record naming a private
//! registry id and an `install_dir` shared by many configs would mark all of
//! them Private, and a public model would then be refused by every one. So the
//! directory match is deliberately narrow — whole-argument equality, and only
//! against a recorded value shaped like a path (`looks_like_a_path`), which is
//! what stops a record claiming `install_dir: "run"` from matching the `run` in every
//! `uv run --directory …` config the marketplace writes. Nothing here is a
//! privilege boundary; it is the difference between a wrong record costing one
//! extension and costing all of them.
//!
//! # Two implementations, one format
//!
//! The writer that matters today is TypeScript
//! (`ui/desktop/src/utils/extensionProvenance.ts`), because the registry id only
//! exists where the marketplace install happens. So [`PROVENANCE_FILE`], the
//! schema version, the field names and the [`name_to_key`] reduction all exist
//! twice, and each side unit-tests its own half against a hand-written fixture
//! rather than a shared one.
//!
//! ⚠ **Drift is silent and it is a DOWNGRADE**, which is the direction that
//! matters: a record the reader cannot find reads as "no provenance", and no
//! provenance returns a renamed entry to the config-name join. It does not throw
//! and it does not warn. The known divergence is deliberate and test-only — the
//! desktop hardcodes `~/.config/biorouter` while this side resolves through
//! [`Paths`], so the `BIOROUTER_PATH_ROOT` seam moves one and not the other.
//! Anything else that diverges is a bug in whichever side moved.
//!
//! # What is stored
//!
//! `<config dir>/extension-provenance.json`, keyed by
//! [`name_to_key`](crate::config::extensions::name_to_key) of the config
//! entry's name — the same reduction the extension manager stores its map keys
//! under, so a lookup cannot miss on spelling:
//!
//! ```json
//! {
//!   "version": 1,
//!   "extensions": {
//!     "cdwagent": {
//!       "registry_id": "cdwagent",
//!       "source_url": "https://github.com/…/cdwagent.brxt",
//!       "bundle_sha256": "…",
//!       "recorded_at": "2026-08-03T19:00:00Z"
//!     }
//!   }
//! }
//! ```
//!
//! [DR-23]: ../../../../docs/security/privacy-tiers-implementation-plan.md

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::config::extensions::name_to_key;
use crate::config::paths::Paths;

/// The file's name inside the config dir. Duplicated in the desktop app's main
/// process (`ui/desktop/src/main.ts`), which is the process that performs a
/// `.brxt` install and therefore the only one that knows the registry id — the
/// same way that handler already hardcodes `~/.config/biorouter/extensions`.
/// The two spellings must agree; `the_store_lives_in_the_config_dir_under_the_documented_name`
/// pins this half and the TypeScript side names this constant in a comment.
pub const PROVENANCE_FILE: &str = "extension-provenance.json";

/// The current on-disk schema version. A file written by a newer build is read
/// **for the fields this build understands** rather than discarded: discarding
/// it would silently drop provenance, and dropping provenance lowers a tier.
const SCHEMA_VERSION: u32 = 1;

/// Where one installed extension came from.
///
/// `registry_id` and `install_dir` participate in the tier decision — the first
/// is what is looked up, the second is one of the two ways the record is found.
/// `source_url` and `bundle_sha256` are recorded because the install already
/// has them and an incident response would not: they are evidence, not inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionProvenance {
    /// The BAAM registry `id` this extension was installed from — the stable
    /// identifier DR-23 keys on. Reduced with `name_to_key` before it is
    /// compared with the compiled snapshot, so either spelling the registry
    /// publishes (`cdwagent` or `CDWAgent`) matches.
    pub registry_id: String,
    /// Where the bundle was unpacked — `~/.config/biorouter/extensions/<name>`.
    ///
    /// ⚠ **This is the field that survives a rename**, and it is the reason the
    /// map key alone is not enough. `config.yaml`'s entry can be renamed after
    /// the install, which changes both the map key and the entry's `name` — but
    /// not the `--directory` argument the install wrote, because moving that
    /// would stop the extension launching at all. See [`registry_ids_for`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_dir: Option<String>,
    /// The URL the `.brxt` was downloaded from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Hex SHA-256 of the downloaded bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<String>,
    /// RFC 3339 timestamp of the install that wrote this record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<String>,
}

/// The non-secret identity needed to validate deletion of one marketplace
/// package. The config key and install directory are captured together so a
/// caller can re-read and compare the same record after user approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceInstallProvenance {
    pub config_key: String,
    pub registry_id: String,
    pub install_dir: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    extensions: HashMap<String, ExtensionProvenance>,
}

/// The store's path: `<config dir>/extension-provenance.json`.
///
/// Resolved through [`Paths`], so the `BIOROUTER_PATH_ROOT` test seam relocates
/// it with the rest of the config root.
pub fn provenance_path() -> PathBuf {
    Paths::in_config_dir(PROVENANCE_FILE)
}

/// Every registry id recorded for an extension now configured under any of
/// `keys`, **and** for any whose recorded `install_dir` appears among
/// `referenced_paths`.
///
/// ⚠ **All matches, not the first one**, because the caller unions them.
/// Returning one record makes the answer depend on which way in matched first,
/// and the ordering that loses is the one that matters: an entry named after a
/// public extension whose arguments point at a private one's install directory
/// would resolve public on a first-match rule and private on this one.
///
/// # Two ways in, because one of them survives a rename and the other does not
///
/// **By key** is the direct hit and covers the common case, including the one
/// where the registry `id` and the installed name already disagree —
/// `spokeagent-0.4.1` does so in today's catalogue.
///
/// **By install directory** is what closes the actual bug. Renaming the entry
/// in `config.yaml` rewrites the map key and the entry's `name`, so a
/// key-only lookup misses and the tier drops to public — which is precisely
/// the enforcement failure DR-23 names. It does *not* rewrite the
/// `--directory` argument the install wrote into `args`, because that path is
/// where the server's code physically lives: repoint it and the extension
/// stops launching. So the install directory is the link a rename cannot
/// break, and `referenced_paths` is the config's own argument list, matched
/// exactly rather than parsed for a flag.
///
/// ⚠ **The honest limit.** Editing `args` to point at a copy of the
/// directory would evade this, and `config.yaml` is agent-writable
/// ([DR-17](privacy-tiers-execution-plan.md) descoped the filesystem
/// barrier). That is a strictly higher bar than renaming a key — it requires
/// relocating the server's code, not editing a label — and evasion still only
/// returns the answer the config-name join would have given, never anything
/// lower.
///
/// A missing, unreadable or malformed store yields nothing rather than
/// erroring. That is safe because of the union in
/// [`super::classify_extension`]: an empty answer falls back to the
/// config-name join, which is exactly the behaviour that shipped before this
/// task.
pub fn registry_ids_for(keys: &[String], referenced_paths: &[String]) -> Vec<String> {
    let mut ids = Vec::new();
    #[cfg(test)]
    {
        let records = test_records().lock().unwrap();
        collect_ids(records.iter(), keys, referenced_paths, &mut ids);
    }
    let store = cached_store_at(&provenance_path());
    collect_ids(store.extensions.iter(), keys, referenced_paths, &mut ids);
    ids
}

/// Installed marketplace packages carrying this exact trusted registry id.
/// Missing install-directory or source-URL evidence is excluded: it is enough
/// for privacy classification, but not enough to authorize package deletion.
pub fn marketplace_installs_for_registry_id(
    registry_id: &str,
) -> Vec<MarketplaceInstallProvenance> {
    marketplace_installs_in_store(&cached_store_at(&provenance_path()), registry_id)
}

fn marketplace_installs_in_store(
    store: &Store,
    registry_id: &str,
) -> Vec<MarketplaceInstallProvenance> {
    let mut installs = store
        .extensions
        .iter()
        .filter_map(|(config_key, record)| {
            if record.registry_id != registry_id {
                return None;
            }
            Some(MarketplaceInstallProvenance {
                config_key: config_key.clone(),
                registry_id: record.registry_id.clone(),
                install_dir: record.install_dir.clone()?,
                source_url: record.source_url.clone()?,
            })
        })
        .collect::<Vec<_>>();
    installs.sort_by(|left, right| left.config_key.cmp(&right.config_key));
    installs
}

/// Remove exactly the record revalidated after approval. A changed record is
/// left intact so a stale approval cannot apply to a replacement install.
pub fn remove_marketplace_install_provenance(
    expected: &MarketplaceInstallProvenance,
) -> std::io::Result<bool> {
    remove_marketplace_install_provenance_at(&provenance_path(), expected)
}

fn remove_marketplace_install_provenance_at(
    path: &Path,
    expected: &MarketplaceInstallProvenance,
) -> std::io::Result<bool> {
    let mut store = read_store_at(path);
    let matches = store
        .extensions
        .get(&expected.config_key)
        .is_some_and(|record| {
            record.registry_id == expected.registry_id
                && record.install_dir.as_deref() == Some(expected.install_dir.as_str())
                && record.source_url.as_deref() == Some(expected.source_url.as_str())
        });
    if !matches {
        return Ok(false);
    }
    store.extensions.remove(&expected.config_key);
    store.version = SCHEMA_VERSION;
    write_store_at(path, &store)?;
    invalidate_cache();
    Ok(true)
}

fn collect_ids<'a>(
    records: impl Iterator<Item = (&'a String, &'a ExtensionProvenance)>,
    keys: &[String],
    referenced_paths: &[String],
    into: &mut Vec<String>,
) {
    for (key, record) in records {
        if keys.iter().any(|k| k == key) || matches_install_dir(record, referenced_paths) {
            into.push(record.registry_id.clone());
        }
    }
}

fn matches_install_dir(record: &ExtensionProvenance, referenced_paths: &[String]) -> bool {
    record
        .install_dir
        .as_deref()
        .is_some_and(|dir| looks_like_a_path(dir) && referenced_paths.iter().any(|p| p == dir))
}

/// A recorded `install_dir` is matched only if it is shaped like a path.
///
/// The arguments this is compared against are a whole command line, not a list
/// of paths — every `.brxt` config is `uv run --directory <dir> <entry>`, so
/// `run`, `--directory` and the entry point are all in it. Without this, a
/// record claiming `install_dir: "run"` would match EVERY marketplace-installed
/// extension at once and lend them all its registry id.
///
/// That is denial rather than disclosure, since the union only ever raises — but
/// a single forged line that refuses every extension to every public model is
/// worth one predicate. The predicate is a separator rather than
/// [`Path::is_absolute`] on purpose: the writer is the Electron main process and
/// may be recording a Windows path this build is not compiled for, and losing a
/// legitimate record IS a downgrade.
fn looks_like_a_path(dir: &str) -> bool {
    dir.contains('/') || dir.contains('\\')
}

/// Record where `config_name` came from, merging into whatever is already on
/// disk.
///
/// ⚠ **No shipped Rust path calls this today, and that is the correct state,
/// not an omission.** The registry id exists only where a marketplace install
/// happens, which is the desktop's Electron main process — it writes this same
/// file through `ui/desktop/src/utils/extensionProvenance.ts`. The CLI's
/// `biorouter extension install` takes a local `.brxt` path, which carries no
/// registry id at all, so it correctly records nothing and leaves the daemon on
/// the config-name join. This exists as the Rust-side writer for the moment a
/// Rust install path *does* learn a registry id (a headless marketplace
/// install, a `--registry-id` flag), and it is what pins the on-disk format
/// from the reading side.
pub fn record(config_name: &str, provenance: ExtensionProvenance) -> std::io::Result<()> {
    record_at(&provenance_path(), config_name, provenance)
}

fn record_at(
    path: &Path,
    config_name: &str,
    provenance: ExtensionProvenance,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut store = read_store_at(path);
    store.version = SCHEMA_VERSION;
    store
        .extensions
        .insert(name_to_key(config_name), provenance);
    write_store_at(path, &store)?;
    invalidate_cache();
    Ok(())
}

fn write_store_at(path: &Path, store: &Store) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "provenance path has no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let body = serde_json::to_vec_pretty(store)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(&body)?;
    tmp.as_file_mut().sync_all()?;
    tmp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// Parse the store at `path`. Any failure — absent, unreadable, not JSON, JSON
/// of the wrong shape — is an empty store. See [`registry_ids_for`] for why
/// that is not a silent downgrade.
fn read_store_at(path: &Path) -> Store {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Store::default();
    };
    match serde_json::from_str::<Store>(&raw) {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "extension provenance store is unreadable; falling back to the config-name join"
            );
            Store::default()
        }
    }
}

/// Stat-keyed cache over [`read_store_at`].
///
/// The resolver runs on hot paths — Gate E iterates every installed extension
/// on every turn, Gate C runs per dispatch — so the file may not be parsed each
/// time. It is `stat`ed each time instead, and re-parsed only when the path,
/// mtime or length changes; the `stat` is also what makes an install visible to
/// an already-running daemon without a restart.
type StatKey = (PathBuf, Option<(SystemTime, u64)>);
type CachedStore = Mutex<Option<(StatKey, Arc<Store>)>>;

fn cache() -> &'static CachedStore {
    static CACHE: OnceLock<CachedStore> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The cache lock, taken so that a poisoned mutex cannot take the daemon with
/// it.
///
/// Nothing inside the critical section can panic today — it is a comparison, a
/// clone and an assignment — so poisoning should be unreachable. But this lock
/// is on the path of every privacy gate, and `unwrap()` here turns a panic
/// somewhere else in the process into a panic in the security resolver on the
/// next tool call. The cached value is a plain snapshot with no invariant a
/// half-finished write could break, so recovering the inner value is always
/// sound; the worst case is one stale read, which the stat key corrects on the
/// call after.
fn lock_cache() -> std::sync::MutexGuard<'static, Option<(StatKey, Arc<Store>)>> {
    cache().lock().unwrap_or_else(|e| e.into_inner())
}

fn invalidate_cache() {
    *lock_cache() = None;
}

fn cached_store_at(path: &Path) -> Arc<Store> {
    // ⚠ A blocking `stat` (and, when the file has changed, a blocking read and
    // parse) on an async gate path — `allowed_extension_keys` calls this once
    // per installed extension that is not already private by name, while it
    // holds the extension map's tokio mutex. That is deliberate and measured
    // rather than overlooked: it is one syscall against a file in the config
    // dir, no `await` is held across the std mutex below, and the alternative —
    // reading the store once per process — is what makes a freshly installed
    // private extension classify Public until the daemon restarts. Freshness is
    // the direction that costs disclosure; latency is not.
    let stamp = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
    let key: StatKey = (path.to_path_buf(), stamp);
    let mut guard = lock_cache();
    if let Some((cached_key, store)) = guard.as_ref() {
        if *cached_key == key {
            return store.clone();
        }
    }
    let store = Arc::new(read_store_at(path));
    *guard = Some((key, store.clone()));
    store
}

/// Additive test provenance, consulted by [`registry_ids_for`] ahead of the
/// file.
///
/// ⚠ Process-global and never cleared, on purpose. Tests in this crate run in
/// parallel in one process, so a store a test could *clear* would be a race; one
/// it can only add to is not.
///
/// ⚠ **What is required of a caller, stated as the rule it actually is.** A
/// record is found by its key OR by its install directory, so:
///
///  * The **install directory** must be unique to the test that writes it.
///    Sharing one lends a registry id to another test's fixture, and because the
///    union only raises, the borrowing test goes green on provenance it did not
///    state. This is the one that bites.
///  * The **key** need not be unique, only consistent: two records under one key
///    overwrite, so two tests that both want `cdwagent` under `cdwagent` agree
///    by construction. Where they would disagree, use distinct names.
///
/// An earlier version of this comment claimed both were unique of every caller,
/// which was false: `privacy::extensions` and `agents::extension_manager` both
/// recorded `…/extensions/CDWAgent`. They agreed on `cdwagent`, so nothing was
/// wrong — but deleting either test would have left the other silently propped
/// up by it, and a comment asserting a property the tests do not hold is worse
/// than no comment. The resolver's fixture now uses a directory of its own, so
/// the rule above is true rather than aspirational.
#[cfg(test)]
fn test_records() -> &'static Mutex<HashMap<String, ExtensionProvenance>> {
    static RECORDS: OnceLock<Mutex<HashMap<String, ExtensionProvenance>>> = OnceLock::new();
    RECORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Pretend the install of `config_name` recorded `registry_id`. See
/// [`test_records`] for why this only ever adds.
#[cfg(test)]
pub(crate) fn insert_test_record(config_name: &str, registry_id: &str) {
    insert_test_record_at(config_name, registry_id, None);
}

/// As [`insert_test_record`], plus the install directory the bundle was
/// unpacked into — the field a later rename cannot touch.
#[cfg(test)]
pub(crate) fn insert_test_record_at(
    config_name: &str,
    registry_id: &str,
    install_dir: Option<&str>,
) {
    test_records().lock().unwrap().insert(
        name_to_key(config_name),
        ExtensionProvenance {
            registry_id: registry_id.to_string(),
            install_dir: install_dir.map(str::to_string),
            source_url: None,
            bundle_sha256: None,
            recorded_at: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join(PROVENANCE_FILE);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn a_record_round_trips_through_the_documented_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            r#"{
              "version": 1,
              "extensions": {
                "mystuff": {
                  "registry_id": "cdwagent",
                  "install_dir": "/home/r/.config/biorouter/extensions/CDWAgent",
                  "source_url": "https://example.invalid/cdwagent.brxt",
                  "bundle_sha256": "abc123",
                  "recorded_at": "2026-08-03T19:00:00Z"
                }
              }
            }"#,
        );
        let store = read_store_at(&path);
        assert_eq!(store.version, 1);
        let record = store.extensions.get("mystuff").expect("the record");
        assert_eq!(record.registry_id, "cdwagent");
        assert_eq!(
            record.install_dir.as_deref(),
            Some("/home/r/.config/biorouter/extensions/CDWAgent")
        );
        assert_eq!(
            record.source_url.as_deref(),
            Some("https://example.invalid/cdwagent.brxt")
        );
        assert_eq!(record.bundle_sha256.as_deref(), Some("abc123"));
    }

    #[test]
    fn marketplace_deletion_evidence_requires_an_exact_complete_record() {
        let mut store = Store::default();
        store.extensions.insert(
            "complete".to_owned(),
            ExtensionProvenance {
                registry_id: "fixture-agent".to_owned(),
                install_dir: Some("/tmp/extensions/FixtureAgent".to_owned()),
                source_url: Some("https://github.com/example/fixture-agent.brxt".to_owned()),
                bundle_sha256: None,
                recorded_at: None,
            },
        );
        store.extensions.insert(
            "missing-url".to_owned(),
            ExtensionProvenance {
                registry_id: "fixture-agent".to_owned(),
                install_dir: Some("/tmp/extensions/Other".to_owned()),
                source_url: None,
                bundle_sha256: None,
                recorded_at: None,
            },
        );

        assert_eq!(
            marketplace_installs_in_store(&store, "fixture-agent"),
            vec![MarketplaceInstallProvenance {
                config_key: "complete".to_owned(),
                registry_id: "fixture-agent".to_owned(),
                install_dir: "/tmp/extensions/FixtureAgent".to_owned(),
                source_url: "https://github.com/example/fixture-agent.brxt".to_owned(),
            }]
        );
        assert!(marketplace_installs_in_store(&store, "FIXTURE-AGENT").is_empty());
    }

    #[test]
    fn stale_marketplace_deletion_evidence_cannot_remove_a_replacement_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PROVENANCE_FILE);
        let expected = MarketplaceInstallProvenance {
            config_key: "fixture".to_owned(),
            registry_id: "fixture-agent".to_owned(),
            install_dir: "/tmp/extensions/FixtureAgent".to_owned(),
            source_url: "https://github.com/example/v1/fixture-agent.brxt".to_owned(),
        };
        record_at(
            &path,
            "fixture",
            ExtensionProvenance {
                registry_id: expected.registry_id.clone(),
                install_dir: Some(expected.install_dir.clone()),
                source_url: Some("https://github.com/example/v2/fixture-agent.brxt".to_owned()),
                bundle_sha256: None,
                recorded_at: None,
            },
        )
        .unwrap();

        assert!(!remove_marketplace_install_provenance_at(&path, &expected).unwrap());
        assert!(read_store_at(&path).extensions.contains_key("fixture"));

        let current = marketplace_installs_in_store(&read_store_at(&path), "fixture-agent")
            .pop()
            .unwrap();
        assert!(remove_marketplace_install_provenance_at(&path, &current).unwrap());
        assert!(read_store_at(&path).extensions.is_empty());
    }

    fn dir_record(install_dir: Option<&str>) -> ExtensionProvenance {
        ExtensionProvenance {
            registry_id: "cdwagent".to_string(),
            install_dir: install_dir.map(str::to_string),
            source_url: None,
            bundle_sha256: None,
            recorded_at: None,
        }
    }

    /// An empty or absent `install_dir` matches nothing.
    ///
    /// A record written by a build that did not have the field, or by a writer
    /// that could not determine it, must not become a wildcard that lends its
    /// registry id to every config with no arguments — which is what a
    /// `Some("") == Some("")` comparison would have produced.
    #[test]
    fn an_empty_install_dir_matches_nothing() {
        for dir in [None, Some("")] {
            let record = dir_record(dir);
            assert!(!matches_install_dir(&record, &[]));
            assert!(!matches_install_dir(&record, &[String::new()]));
            assert!(!matches_install_dir(&record, &["run".to_string()]));
        }
    }

    /// **A record cannot claim a bare word and match every extension at once.**
    ///
    /// The list it is compared against is a whole command line, not a list of
    /// paths: every `.brxt` config is `uv run --directory <dir> <entry>`, so
    /// `run`, `--directory` and `server.py` are all in it. A forged record
    /// claiming `install_dir: "run"` would therefore have matched EVERY
    /// marketplace-installed extension and lent them all its registry id.
    ///
    /// That is denial rather than disclosure — the union only ever raises, so
    /// the result is refusals, not leaks — but one forged line refusing every
    /// extension to every public model is worth a predicate. A real recorded
    /// directory still matches, in both platforms' spellings, because losing a
    /// legitimate record IS a downgrade.
    #[test]
    fn a_recorded_install_dir_that_is_not_a_path_matches_nothing() {
        let uv_args = [
            "run".to_string(),
            "--directory".to_string(),
            "/home/r/.config/biorouter/extensions/CDWAgent".to_string(),
            "server.py".to_string(),
        ];
        for bare in ["run", "--directory", "server.py", "uv", "."] {
            assert!(
                !matches_install_dir(&dir_record(Some(bare)), &uv_args),
                "`{bare}` was accepted as an install directory, so one forged record marks \
                 every uv-launched extension private"
            );
        }
        for real in [
            "/home/r/.config/biorouter/extensions/CDWAgent",
            "C:\\Users\\r\\.config\\biorouter\\extensions\\CDWAgent",
        ] {
            assert!(
                matches_install_dir(&dir_record(Some(real)), &[real.to_string()]),
                "a real recorded directory stopped matching, which loses the record and \
                 downgrades the entry: {real}"
            );
        }
    }

    /// Only `registry_id` is required. The desktop writes the other three; a
    /// future writer that cannot compute a hash must still produce a record
    /// that resolves, because a record that fails to parse is a downgrade.
    #[test]
    fn the_evidence_fields_are_optional() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            r#"{"version":1,"extensions":{"mystuff":{"registry_id":"cdwagent"}}}"#,
        );
        assert_eq!(
            read_store_at(&path).extensions["mystuff"].registry_id,
            "cdwagent"
        );
    }

    /// A file from a newer build keeps the fields this build understands. The
    /// alternative — refusing to read an unknown `version` — drops provenance,
    /// and dropping provenance lowers a tier.
    #[test]
    fn a_newer_schema_version_is_read_rather_than_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            r#"{"version":99,"extensions":{"mystuff":{"registry_id":"cdwagent","future":1}},"whatever":true}"#,
        );
        assert_eq!(
            read_store_at(&path).extensions["mystuff"].registry_id,
            "cdwagent"
        );
    }

    #[test]
    fn an_absent_or_malformed_store_is_empty_rather_than_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_store_at(&dir.path().join(PROVENANCE_FILE))
            .extensions
            .is_empty());
        let path = write(dir.path(), "{ this is not json");
        assert!(read_store_at(&path).extensions.is_empty());
        let path = write(dir.path(), r#"{"version":1,"extensions":[]}"#);
        assert!(read_store_at(&path).extensions.is_empty());
    }

    /// The cache is keyed on the file's stat, so an install performed while the
    /// daemon is running is visible on the next lookup rather than at the next
    /// restart. Without this the store would be read once per process and a
    /// freshly installed private extension would classify public until relaunch.
    #[test]
    fn a_rewritten_store_is_re_read_rather_than_served_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            r#"{"version":1,"extensions":{"mystuff":{"registry_id":"playwrightagent"}}}"#,
        );
        assert_eq!(
            cached_store_at(&path).extensions["mystuff"].registry_id,
            "playwrightagent"
        );
        // Same path, different contents. `len` differs here, and mtime differs
        // on any filesystem with sub-second timestamps; the key carries both.
        write(
            dir.path(),
            r#"{"version":1,"extensions":{"mystuff":{"registry_id":"cdwagent","source_url":"x"}}}"#,
        );
        assert_eq!(
            cached_store_at(&path).extensions["mystuff"].registry_id,
            "cdwagent"
        );
    }

    #[test]
    fn the_store_lives_in_the_config_dir_under_the_documented_name() {
        let path = provenance_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some(PROVENANCE_FILE)
        );
        assert_eq!(path.parent(), Some(Paths::config_dir().as_path()));
    }

    /// The writer and the reader agree, and a second install does not orphan
    /// the first record. The same two properties are asserted on the
    /// TypeScript writer (`extensionProvenance.test.ts`), because the desktop
    /// is what actually writes this file today and the two implementations have
    /// to produce one format.
    #[test]
    fn the_writer_round_trips_and_preserves_what_an_earlier_install_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PROVENANCE_FILE);

        record_at(
            &path,
            "CDWAgent",
            ExtensionProvenance {
                registry_id: "cdwagent".to_string(),
                install_dir: Some("/home/r/.config/biorouter/extensions/CDWAgent".to_string()),
                source_url: Some("https://example.invalid/cdwagent.brxt".to_string()),
                bundle_sha256: None,
                recorded_at: Some("2026-08-03T19:00:00Z".to_string()),
            },
        )
        .unwrap();
        record_at(
            &path,
            "My Renamed Connector",
            ExtensionProvenance {
                registry_id: "ucsfomopagent".to_string(),
                install_dir: None,
                source_url: None,
                bundle_sha256: None,
                recorded_at: None,
            },
        )
        .unwrap();

        let store = read_store_at(&path);
        assert_eq!(store.version, SCHEMA_VERSION);
        // Keyed on the reduced CONFIG name, which is what diverges from the
        // registry id and is the entire reason this file exists.
        assert_eq!(store.extensions["cdwagent"].registry_id, "cdwagent");
        assert_eq!(
            store.extensions["myrenamedconnector"].registry_id,
            "ucsfomopagent"
        );
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file left behind"
        );
    }

    /// The lookup key is the extension manager's map key, not the raw config
    /// name, or a record written for `CDWAgent` would never be found for the
    /// entry the manager stores under `cdwagent`.
    #[test]
    fn a_record_is_found_under_the_reduced_key() {
        insert_test_record("Provenance Key Fixture", "cdwagent");
        assert_eq!(
            registry_ids_for(&[name_to_key("  provenancekeyfixture ")], &[]),
            vec!["cdwagent".to_string()]
        );
    }

    /// **Every match, not the first.** An entry named after a public extension
    /// whose arguments point at a private one's install directory must yield
    /// both ids, or the caller's union silently becomes first-match-wins and
    /// the losing order is the one that lets a private connector through.
    #[test]
    fn a_key_match_does_not_mask_an_install_directory_match() {
        let dir = "/home/researcher/.config/biorouter/extensions/CollisionFixture";
        insert_test_record("collision-fixture-public", "playwrightagent");
        insert_test_record_at("collision-fixture-private", "cdwagent", Some(dir));

        let ids = registry_ids_for(
            &[name_to_key("collision-fixture-public")],
            &[dir.to_string()],
        );
        assert!(ids.contains(&"playwrightagent".to_string()), "{ids:?}");
        assert!(ids.contains(&"cdwagent".to_string()), "{ids:?}");
    }
}
