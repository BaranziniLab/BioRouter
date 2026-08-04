//! Where the master privacy switch is **recorded** (issue #56, DR-22).
//!
//! The value itself is [`biorouter_mcp::privacy_toggle`]'s process-global
//! atomic, and it still has exactly two writers — start-up's
//! [`super::load_privacy_tiers_from_config`] and `/config/upsert`'s gated arm.
//! This module is only the disk half: what start-up reads, and what the gated
//! arm writes.
//!
//! ⚠ **Why not `config.yaml`, which is where it used to live.** Task 30 closed
//! the HTTP channel — a bare `/config/upsert` of the key is refused, and the
//! confirmed one carries a typed phrase — but the value's home was a file, and
//! [DR-17] descoped the filesystem barrier that DR-14 had put around
//! `config.yaml`. So *writing the key into the file by hand* stayed a
//! **next-launch** disable of the control the agent is subject to, and "only on
//! restart" is not a control: daemons restart routinely and a model can simply
//! wait. DR-22 moves the value somewhere the retired key cannot reach it.
//!
//! ⚠ **What that does and does not buy, stated plainly, because the honest
//! bound matters more than the flattering one.**
//!
//! * It **closes the key**. `BIOROUTER_PRIVACY_TIERS: off` in `config.yaml` is
//!   inert from the migration onward — the documented, discoverable, widely
//!   quoted recipe (it is in this repo's own docs, in the design, and therefore
//!   in every model's memory of it) no longer does anything. That is the channel
//!   DR-22 names.
//! * It gives the value **one writer**. `config.yaml` is written by nearly every
//!   settings interaction, so a write to it is unremarkable; nothing else in the
//!   tree writes this file, so a write to it is the switch moving and nothing
//!   else.
//! * It does **not** make the file unwritable. DR-17 left every file in the
//!   config directory reachable by an agent holding `developer__shell`, and this
//!   one is no different — the same residual [`super::disclosure`]'s
//!   acknowledgement record carries, and it is recorded here rather than left
//!   for a reader to discover. Closing it needs the filesystem barrier DR-17
//!   deferred, or an OS-authenticated store; neither is in v1, and this module
//!   must not be cited as though it were either.
//!
//! ⚠ **The store is created even when the answer is the default**, and that is
//! load-bearing rather than tidy. Its existence is the migration's "already
//! done" marker (see [`migrate_once`]), and on the overwhelming majority of
//! installs the answer *is* the default — so a store written only on `off` would
//! leave the migration live for ever on almost every machine, which is to say it
//! would leave the retired key live for ever on almost every machine.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// The record's filename, beside `config.yaml` in the configuration directory.
pub const SWITCH_FILE_NAME: &str = "privacy-tiers.json";

/// What is written when the switch moves.
///
/// A timestamp beside the flag for the same reason
/// [`super::disclosure::Acknowledgement`] carries one: "when did this machine
/// stop enforcing" is the first question anyone auditing an incident asks, and a
/// bare boolean cannot answer it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterSwitchRecord {
    /// `true` — the default — means every gate and the classification ratchet
    /// are live.
    pub enabled: bool,
    /// RFC 3339, in UTC. `default` so a hand-written `{"enabled": false}` still
    /// reads: this field is an audit aid, not a checksum, and refusing a record
    /// for missing it would fail towards *on* in a way the user did not ask for.
    #[serde(default)]
    pub changed_at: String,
}

/// The directory the record lives in: the one holding `config.yaml`.
///
/// ⚠ **Derived from [`Config::path`], not from
/// [`crate::config::paths::Paths::config_dir`].** `Config::global()` is a
/// `OnceCell` that resolves its path on first access and keeps it, while
/// `Paths::config_dir()` re-reads `BIOROUTER_PATH_ROOT` on every call — so in
/// any process where that variable moves after the config was first touched
/// (every integration-test binary in this tree), the two answer differently.
/// The migration reads one file and writes another; if those two could be
/// resolved from different roots it would migrate across installs. Following
/// the `Config` makes them the same directory by construction, with no second
/// environment read to keep in step.
fn dir_of(config: &Config) -> PathBuf {
    Path::new(&config.path())
        .parent()
        .map(Path::to_path_buf)
        // A `Config` whose path has no parent is not reachable through any
        // constructor in this tree; the current directory keeps this total
        // rather than panicking inside a start-up path.
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where the record is, for a given configuration directory.
pub fn path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(SWITCH_FILE_NAME)
}

/// Where the record is, for a given [`Config`].
pub fn path_for(config: &Config) -> PathBuf {
    path_in(&dir_of(config))
}

/// The recorded answer, or `None` if this install has not recorded one.
///
/// ⚠ **Fail-safe means fail towards enforcing.** Absent, unreadable and
/// malformed all read `None`, and every caller resolves that to ON — the same
/// polarity the loader has always had, and for the same reason: the failure of
/// the reader must not be a way to disable the control.
///
/// A malformed record is deliberately *not* repaired here. `None` already
/// enforces, and rewriting a file this function is only supposed to read would
/// make a reader into a third writer.
pub fn read_in(config_dir: &Path) -> Option<bool> {
    let raw = std::fs::read_to_string(path_in(config_dir)).ok()?;
    serde_json::from_str::<MasterSwitchRecord>(&raw)
        .ok()
        .map(|record| record.enabled)
}

/// [`read_in`], for the directory a given [`Config`] lives in.
pub fn read_for(config: &Config) -> Option<bool> {
    read_in(&dir_of(config))
}

/// Has this install recorded an answer at all? The migration's "already done"
/// marker.
///
/// Deliberately the file's **existence** and not `read_in(..).is_some()`: a
/// record that fails to parse must not re-open the migration, or a single
/// corrupt byte would make the retired key live again.
pub fn exists_in(config_dir: &Path) -> bool {
    path_in(config_dir).exists()
}

/// Record the answer.
///
/// ⚠ **Staged and renamed, never written in place** — the same hazard
/// [`super::disclosure::record_acknowledgement_in`] documents. `fs::write` opens
/// with `truncate`, so between the truncate and the write the record on disk is
/// empty, and an empty record reads as *nothing recorded*, which resolves to ON.
/// A process that dies inside that window would silently re-enable a feature the
/// user turned off. A rename within one directory is atomic, so the record is
/// only ever absent or complete.
pub fn write_in(config_dir: &Path, enabled: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let record = MasterSwitchRecord {
        enabled,
        changed_at: chrono::Utc::now().to_rfc3339(),
    };
    let body = serde_json::to_string_pretty(&record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Named per process, so two Biorouter processes writing at the same moment
    // stage into different files and each rename lands a complete record.
    let staging = config_dir.join(format!("{SWITCH_FILE_NAME}.{}.tmp", std::process::id()));
    std::fs::write(&staging, body)?;
    match std::fs::rename(&staging, path_in(config_dir)) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Do not leave the staging file in the user's config directory.
            let _ = std::fs::remove_file(&staging);
            Err(e)
        }
    }
}

/// [`write_in`], for the directory a given [`Config`] lives in.
pub fn write_for(config: &Config, enabled: bool) -> std::io::Result<()> {
    write_in(&dir_of(config), enabled)
}

/// Carry a pre-DR-22 `config.yaml` value into the store, **once**, and retire
/// the key. Returns whether it did anything.
///
/// ⚠ **This function contains the only read of the retired key in the tree**,
/// and that is the whole of Step 2. A reader that still consults `config.yaml`
/// has not closed the channel, it has added a second one — so the key is
/// *ignored*, not read-and-overridden and not honoured "for compatibility".
///
/// ⚠ **Gated on the STORE's existence, never on the key's.** "Migrate whenever
/// the key is present" is the same compatibility reader wearing a different hat:
/// it would re-run every time the key reappeared, which is exactly the write an
/// agent would make. Because the store is written even for the default answer,
/// one start-up after the upgrade is enough to close the door for good.
///
/// ⚠ **Write first, delete second.** If the store cannot be written — an
/// unwritable configuration directory — the key stays where it is and this
/// returns `false`, so the user's answer is preserved for the next attempt
/// rather than destroyed by a half-finished migration. The interim resolves to
/// ON, which is the safe direction and is what a failed read has always meant.
pub fn migrate_once(config: &Config) -> bool {
    let dir = dir_of(config);
    if exists_in(&dir) {
        return false;
    }
    // Read from the loaded values map, NEVER through `Config::get_param`, whose
    // middle branch resolves an environment variable — the agent holds
    // `developer__shell`, so an env-readable value would make
    // `BIOROUTER_PRIVACY_TIERS=off biorouterd` a one-token disable, and a
    // migration that honoured the environment would hand that lever back on the
    // one start-up where it still mattered.
    let carried = config
        .all_values()
        .ok()
        .and_then(|values| values.get(super::PRIVACY_TIERS_CONFIG_KEY).cloned())
        .and_then(|value| super::privacy_tiers_value_is_on(&value));
    if write_in(&dir, carried.unwrap_or(true)).is_err() {
        return false;
    }
    // `delete` on an absent key is not an error worth reporting: the common case
    // is a fresh install that never had one, and the store is already written.
    let _ = config.delete(super::PRIVACY_TIERS_CONFIG_KEY);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().expect("temp config dir");
        let config = Config::new_with_file_secrets(
            dir.path().join("config.yaml"),
            dir.path().join("secrets.yaml"),
        )
        .expect("scratch config");
        (dir, config)
    }

    #[test]
    fn nothing_recorded_reads_as_nothing_recorded() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_in(dir.path()), None);
        assert!(!exists_in(dir.path()));
    }

    #[test]
    fn the_record_round_trips_in_both_positions() {
        let dir = tempfile::tempdir().unwrap();
        write_in(dir.path(), false).unwrap();
        assert_eq!(read_in(dir.path()), Some(false));
        assert!(exists_in(dir.path()));
        write_in(dir.path(), true).unwrap();
        assert_eq!(read_in(dir.path()), Some(true));
    }

    /// Fail-safe means fail towards enforcing: a truncated or scribbled-on
    /// record must read as *nothing recorded*, which every caller resolves to
    /// ON. The opposite polarity would make corrupting one file a way to
    /// disable the feature.
    #[test]
    fn a_malformed_record_reads_as_nothing_recorded_but_still_blocks_the_migration() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path_in(dir.path()), "{").unwrap();
        assert_eq!(read_in(dir.path()), None);
        assert!(
            exists_in(dir.path()),
            "a corrupt record must not re-open the migration — one bad byte would \
             otherwise make the retired key live again"
        );
    }

    /// A record written by hand with only the flag must read. `changed_at` is
    /// an audit aid; refusing a record for missing it would fail towards ON in a
    /// way the user did not ask for.
    #[test]
    fn a_record_without_the_timestamp_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path_in(dir.path()), r#"{"enabled": false}"#).unwrap();
        assert_eq!(read_in(dir.path()), Some(false));
    }

    #[test]
    fn the_store_sits_beside_the_config_file_it_migrates_from() {
        let (dir, config) = scratch();
        assert_eq!(path_for(&config), dir.path().join(SWITCH_FILE_NAME));
    }

    #[test]
    fn the_migration_carries_the_retired_value_across_and_removes_the_key() {
        let (_dir, config) = scratch();
        config
            .set(
                super::super::PRIVACY_TIERS_CONFIG_KEY,
                &serde_json::Value::String("off".to_string()),
                false,
            )
            .unwrap();

        assert!(migrate_once(&config));
        assert_eq!(read_for(&config), Some(false));
        assert!(
            !config
                .all_values()
                .unwrap()
                .contains_key(super::super::PRIVACY_TIERS_CONFIG_KEY),
            "the migration must remove the key, not leave it beside the store to drift"
        );
    }

    /// The default answer is recorded too, and that is what stops the migration
    /// running again on the ~all installs that never disabled anything.
    #[test]
    fn an_install_that_never_set_the_key_still_gets_a_store() {
        let (_dir, config) = scratch();
        assert!(migrate_once(&config));
        assert_eq!(read_for(&config), Some(true));
        assert!(!migrate_once(&config), "the migration runs once");
    }

    /// The failure this closes: writing the key back after the migration must
    /// not migrate a second time.
    #[test]
    fn the_key_written_back_after_the_migration_is_ignored() {
        let (_dir, config) = scratch();
        assert!(migrate_once(&config));
        config
            .set(
                super::super::PRIVACY_TIERS_CONFIG_KEY,
                &serde_json::Value::String("off".to_string()),
                false,
            )
            .unwrap();
        assert!(!migrate_once(&config));
        assert_eq!(
            read_for(&config),
            Some(true),
            "the retired key was read a second time — 'once, at migration' means once"
        );
    }

    /// An environment variable must not reach the one read of the retired key
    /// either. `Config::get_param` resolves env before the file; the migration
    /// reads the values map instead, so this stays true on the single start-up
    /// where the key is still consulted at all.
    #[test]
    #[serial_test::serial]
    fn no_environment_variable_can_reach_the_migration() {
        let (_dir, config) = scratch();
        let _env = env_lock::lock_env([(super::super::PRIVACY_TIERS_CONFIG_KEY, Some("off"))]);
        assert!(migrate_once(&config));
        assert_eq!(
            read_for(&config),
            Some(true),
            "the environment reached the migration's read of the retired key"
        );
    }
}
