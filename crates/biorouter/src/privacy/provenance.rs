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
//! # Why this file needs no gated writer
//!
//! Because the resolver **unions** rather than replaces. `classify_extension`
//! returns Private when *either* the config name or the recorded registry id is
//! in the compiled marketplace snapshot, so a record can only ever RAISE a
//! tier. Forging one, corrupting the file, or deleting it outright cannot lower
//! anything below what the name alone already said — which is precisely
//! DR-23's own argument that re-deriving *removes* the problem instead of
//! guarding it. Sessions have exactly one gated writer because a session's tier
//! is stored; an extension's is not, and this file stores identity, not tier.
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
    /// would stop the extension launching at all. See [`find`].
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
        .is_some_and(|dir| !dir.is_empty() && referenced_paths.iter().any(|p| p == dir))
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
    let body = serde_json::to_string_pretty(&store)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Write-then-rename, so a crash mid-write leaves the previous store intact
    // rather than a truncated one. A truncated store reads as "no provenance",
    // which is a downgrade for every renamed entry in it.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)?;
    invalidate_cache();
    Ok(())
}

/// Parse the store at `path`. Any failure — absent, unreadable, not JSON, JSON
/// of the wrong shape — is an empty store. See [`lookup`] for why that is not a
/// silent downgrade.
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

fn invalidate_cache() {
    *cache().lock().unwrap() = None;
}

fn cached_store_at(path: &Path) -> Arc<Store> {
    let stamp = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
    let key: StatKey = (path.to_path_buf(), stamp);
    let mut guard = cache().lock().unwrap();
    if let Some((cached_key, store)) = guard.as_ref() {
        if *cached_key == key {
            return store.clone();
        }
    }
    let store = Arc::new(read_store_at(path));
    *guard = Some((key, store.clone()));
    store
}

/// Additive test provenance, consulted by [`find`] ahead of the file.
///
/// ⚠ Process-global and never cleared, on purpose. Tests in this crate run in
/// parallel in one process, so a store a test could *clear* would be a race;
/// one it can only add a uniquely-named entry to is not. Every caller therefore
/// uses a config name and an install directory no other test uses.
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

    /// An empty or absent `install_dir` matches nothing.
    ///
    /// A record written by a build that did not have the field, or by a writer
    /// that could not determine it, must not become a wildcard that lends its
    /// registry id to every config with no arguments — which is what a
    /// `Some("") == Some("")` comparison would have produced.
    #[test]
    fn an_empty_install_dir_matches_nothing() {
        for dir in [None, Some(String::new())] {
            let record = ExtensionProvenance {
                registry_id: "cdwagent".to_string(),
                install_dir: dir,
                source_url: None,
                bundle_sha256: None,
                recorded_at: None,
            };
            assert!(!matches_install_dir(&record, &[]));
            assert!(!matches_install_dir(&record, &[String::new()]));
            assert!(!matches_install_dir(&record, &["run".to_string()]));
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
