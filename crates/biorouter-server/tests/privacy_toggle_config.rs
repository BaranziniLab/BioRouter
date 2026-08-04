//! Task 30: `/config/upsert` is the master toggle's second writer, and the ONE
//! key it will not write as an ordinary configuration value (issue #56, DR-15).
//!
//! ⚠ **Why this is an integration binary and not a `#[cfg(test)] mod` beside the
//! route.** The toggle is a PROCESS-GLOBAL atomic and `cargo test` runs a
//! crate's unit tests in parallel threads of one process — and
//! `crates/biorouter-server/src/routes/` is compiled into TWO of them, the lib
//! and the `biorouterd` bin. Written next to the route, these tests' OFF window
//! disabled the barrier under
//! `routes::knowledge::tests::a_public_model_is_refused_another_sessions_private_conversation_with_409`
//! and
//! `routes::apps::tests::privacy_capability::the_app_capability_report_follows_the_manifests_provider_not_the_global_one`,
//! which do not take the fixture and cannot be made to — there are ~550 tests
//! across the two targets. Measured, not predicted: both failed on the first
//! full-workspace run. Each `tests/*.rs` file is its own process, so nothing
//! outside this file can observe its writes.

use axum::Json;
use biorouter::config::Config;
use biorouter_server::routes::config_management::{
    read_all_config, read_config, remove_config, upsert_config, ConfigKeyQuery,
    ConfigValueResponse, UpsertConfigQuery,
};
use http::{HeaderMap, StatusCode};
use serde_json::Value;

/// Points `Config::global()` at a throwaway directory, and restores the
/// process-global toggle on drop — including on the unwind path a failing
/// assertion takes.
///
/// ⚠ **These tests must never write the developer's real `config.yaml`, and an
/// earlier version of them did.** `upsert_config` persists, so the accepting arm
/// really does write a file; capturing and restoring the one key it touches
/// looked sufficient and was not. `Config::global()`'s guard is a mutex *inside
/// one process*, and the same tests were compiled into two targets — the lib and
/// the `biorouterd` bin — which `cargo test` runs as separate processes. Their
/// restores interleaved and left `BIOROUTER_PRIVACY_TIERS: off` in the real
/// config, which then made an unrelated test in another crate fail for a reason
/// that had nothing to do with its own code. Redirecting the whole config root
/// is the fix: there is no shared file left to race over.
///
/// ⚠ **ONE root for the whole process, and it is never dropped.**
/// `Config::global()` is a `OnceCell`: it resolves its path on FIRST access and
/// keeps it. A per-test `TempDir` would therefore work for the first test and
/// then be deleted out from under the second, which would still be writing to
/// it. A `OnceLock<TempDir>` in a static is the right lifetime — statics do not
/// drop, so the directory outlives every test, and the OS reclaims it.
///
/// `set_var` rather than `env_lock`: `EnvGuard` borrows its keys and so cannot
/// be stored beside the value it protects, and there is nothing here to race —
/// this is an integration binary whose only two tests are `#[serial]`.
static CONFIG_ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();

struct PrivacyToggleFixture {
    prev_enabled: bool,
}

impl PrivacyToggleFixture {
    fn capture() -> Self {
        let root = CONFIG_ROOT.get_or_init(|| {
            let dir = tempfile::tempdir().expect("temp config root");
            std::fs::create_dir_all(dir.path().join("config")).expect("config dir");
            dir
        });
        std::env::set_var(
            "BIOROUTER_PATH_ROOT",
            root.path().to_str().expect("utf-8 temp path"),
        );
        // Fail loudly rather than silently writing the developer's real file:
        // if `Config::global()` was somehow initialised before this ran, every
        // assertion below would still pass while the side effect landed in
        // `~/.config/biorouter/config.yaml`.
        assert!(
            Config::global().all_values().is_ok_and(|_| true)
                && biorouter::config::paths::Paths::config_dir().starts_with(root.path()),
            "the config root was not redirected; refusing to write the real config.yaml"
        );
        Self {
            prev_enabled: biorouter::privacy::privacy_tiers_enabled(),
        }
    }
}

impl Drop for PrivacyToggleFixture {
    fn drop(&mut self) {
        biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(self.prev_enabled);
    }
}

fn upsert(key: &str, value: &str, confirm: Option<&str>) -> UpsertConfigQuery {
    UpsertConfigQuery {
        key: key.to_string(),
        value: Value::String(value.to_string()),
        is_secret: false,
        confirm: confirm.map(str::to_string),
    }
}

/// ⚠ **THESE ASSERTIONS LOOK CONTRADICTORY AND ARE NOT.** `/config/upsert` MUST
/// be one of the toggle's two writers — it is the channel Settings > Privacy
/// uses — and a BARE upsert of this key MUST be refused. What separates them is
/// the confirmation field, which is what the panel sends and what a tool call
/// composing an ordinary config write does not.
///
/// What this is and what it is not: a **UX guard against an accidental or
/// model-composed config write**, not an authorization boundary. The phrase is a
/// fixed string in the shipped source, so a caller holding the daemon secret
/// replays it — accepted for the same reason AR-15 is, because `check_token` has
/// no principal and such a caller can raise its own session's capability anyway.
/// ⚠ `#[serial]`, and the other test in this file carries it too. Moving these
/// out of the lib stopped them disturbing ~550 unrelated tests; it did not stop
/// them disturbing EACH OTHER. Both mutate the same process-global atomic and
/// both write the same config key, and `cargo test` runs a binary's two tests in
/// two threads.
#[tokio::test]
#[serial_test::serial]
async fn a_bare_config_upsert_cannot_flip_the_key_but_the_confirmed_one_can() {
    let _fixture = PrivacyToggleFixture::capture();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(true);
    let headers = HeaderMap::new();

    let (status, body) = upsert_config(
        headers.clone(),
        Json(upsert(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            "off",
            None,
        )),
    )
    .await
    .expect_err("a bare upsert of the master switch must be refused");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body.contains("Settings"),
        "the refusal must name the way in: {body}"
    );
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "a refused request must not have written"
    );

    // A wrong phrase is refused, and the comparison is EXACT — this is the
    // assertion that fails an `eq_ignore_ascii_case` or a `trim()`.
    let (status, _) = upsert_config(
        headers.clone(),
        Json(upsert(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            "off",
            Some("disable privacy tiers"),
        )),
    )
    .await
    .expect_err("a wrong phrase must be refused");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(biorouter::privacy::privacy_tiers_enabled());

    // …and the confirmed one goes through, and moves the LIVE value rather than
    // only the file: the authoritative copy is in daemon memory, so a handler
    // that wrote config.yaml and stopped would leave every gate enforcing until
    // the next restart.
    let _ok = upsert_config(
        headers,
        Json(upsert(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            "off",
            Some(biorouter::privacy::PRIVACY_TIERS_DISABLE_PHRASE),
        )),
    )
    .await
    .expect("the confirmed flip is the one write this route allows");
    assert!(!biorouter::privacy::privacy_tiers_enabled());
}

/// The two asymmetries review found beside the confirmed arm above. Neither was
/// a hole — both land in the safe direction — and both are closed because the
/// value of "there is exactly one way this changes" is that it is TRUE, not that
/// every way it is false happens to be harmless.
///
/// ⚠ `#[serial]` for the same reason as its two siblings: one process-global
/// atomic, one shared temp config file, and `cargo test` runs a binary's tests
/// in threads.
#[tokio::test]
#[serial_test::serial]
async fn the_master_switch_has_exactly_one_door_and_it_is_not_delete_or_the_secret_store() {
    let _fixture = PrivacyToggleFixture::capture();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(true);
    let headers = HeaderMap::new();

    // DELETE. `is_capability_key`'s own doc argues one predicate for both verbs;
    // this key had the guard on `upsert` alone, so a `/config/remove` took the
    // key off disk — the next boot then reads *absent* and resolves to ON while
    // the running daemon keeps whatever its atomic held.
    Config::global()
        .set(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            &Value::String("off".to_string()),
            false,
        )
        .unwrap();
    let (status, body) = remove_config(
        headers.clone(),
        Json(ConfigKeyQuery {
            key: biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY.to_string(),
            is_secret: false,
        }),
    )
    .await
    .expect_err("a delete of the master switch must be refused");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body.contains("Settings"),
        "the refusal must name the way in: {body}"
    );
    assert!(
        Config::global()
            .get_param::<Value>(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY)
            .is_ok(),
        "a refused delete must not have removed the key"
    );

    // THE SECRET STORE. `config.set(.., is_secret)` routes to the OS credential
    // store, which the loader's `all_values()` does not read — so a CONFIRMED
    // secret write would set this process's atomic to `off` and then silently
    // revert to `on` at the next launch. The panel always sends `false`, so this
    // is unreachable today; it is refused so that stays a property of the daemon
    // rather than of one caller.
    let (status, body) = upsert_config(
        headers,
        Json(UpsertConfigQuery {
            key: biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY.to_string(),
            value: Value::String("off".to_string()),
            is_secret: true,
            confirm: Some(biorouter::privacy::PRIVACY_TIERS_DISABLE_PHRASE.to_string()),
        }),
    )
    .await
    .expect_err("the master switch must not be written to the secret store");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("secret"), "{body}");
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "a refused request must not have moved the live value"
    );
}

/// The renderer mirrors [`biorouter::privacy::privacy_tiers_value_is_on`] in
/// `settings/privacy/privacyTiers.ts`, and the two must agree on surrounding
/// whitespace or a hand-edited `BIOROUTER_PRIVACY_TIERS: " off "` renders as off
/// in Settings → Privacy while the daemon goes on enforcing. Telling the user
/// something false about the control they just used is the one failure the
/// parser's own doc-comment says to avoid.
#[test]
fn the_value_parser_agrees_with_the_renderer_about_whitespace() {
    let is_on = |s: &str| biorouter::privacy::privacy_tiers_value_is_on(&Value::String(s.into()));
    assert_eq!(is_on(" off "), Some(false));
    assert_eq!(is_on("\tFalse\n"), Some(false));
    assert_eq!(is_on(" no "), Some(false));
    assert_eq!(is_on(" on "), Some(true));
    // A shape that is neither a bool nor a string still says "I cannot tell",
    // which every caller resolves to ON.
    assert_eq!(
        biorouter::privacy::privacy_tiers_value_is_on(&Value::Null),
        None
    );
}

/// The other half of hardening measure (1): the value the daemon boots with
/// comes from a FILE the daemon owns, and an environment variable cannot reach
/// it.
///
/// ⚠ **Task 42 re-pointed the second half of this test, and that is the ruling,
/// not a weakening.** It used to write the retired key into `config.yaml` and
/// assert the loader honoured it — *"the FILE is what the loader reads"*.
/// [DR-22] says that file must no longer be the switch's home, so an assertion
/// that `config.yaml` still steers the loader is now an assertion that the
/// ruling was not implemented. The property it exists to protect — no
/// environment variable can turn protection off, and the boot value comes from
/// disk rather than from the process environment — is asserted unchanged, and
/// now against the store the loader really reads.
#[tokio::test]
#[serial_test::serial]
async fn the_startup_load_reads_the_switch_store_and_not_the_environment() {
    // ⚠ The fixture's env guard FIRST — it is what redirects the config root,
    // and taking a second `lock_env` while it is held would deadlock. This one
    // adds the variable under test to the same process environment by a plain
    // `set_var`, because `env_lock`'s guard is not reentrant and the fixture
    // already owns it.
    let _fixture = PrivacyToggleFixture::capture();
    struct EnvVar(&'static str);
    impl Drop for EnvVar {
        fn drop(&mut self) {
            std::env::remove_var(self.0);
        }
    }
    std::env::set_var(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY, "off");
    let _unset = EnvVar(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY);

    // The other tests in this binary write both the key and the store into the
    // shared temp config root, and `#[serial]` orders them but says nothing
    // about which runs first. Clear both so "nothing recorded" means exactly
    // that whichever order they take.
    reset_switch_storage();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(false);
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "nothing recorded must resolve to ON, and the env var must not be consulted"
    );

    // The record is written here as BYTES rather than through the writer, so
    // this pins the on-disk format a user's backup and a support answer depend
    // on — a round-trip through `write_for` would agree with itself whatever it
    // wrote. `changed_at` is deliberately omitted: it is an audit field, and a
    // record missing it must still read.
    std::fs::write(
        config_dir().join("privacy-tiers.json"),
        r#"{"enabled": false}"#,
    )
    .unwrap();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(true);
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        !biorouter::privacy::privacy_tiers_enabled(),
        "the switch STORE is what the loader reads"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Task 42 (DR-22): the master switch's storage is not `config.yaml`.
// ─────────────────────────────────────────────────────────────────────────────

/// The directory `Config::global()` resolved its `config.yaml` into — which is
/// also where the switch store lives, by construction rather than by a second
/// env read (see `biorouter::privacy::master_switch`).
fn config_dir() -> std::path::PathBuf {
    std::path::Path::new(&Config::global().path())
        .parent()
        .expect("config.yaml always has a parent directory")
        .to_path_buf()
}

/// Return this install to its pre-migration state: no store, no retired key.
/// The tests below each start from it, because the fixture's temp config root
/// is shared by every test in this binary and `#[serial]` fixes no order.
fn reset_switch_storage() {
    std::fs::remove_file(config_dir().join("privacy-tiers.json")).ok();
    Config::global()
        .delete(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY)
        .ok();
}

/// Is the retired key present in the config FILE? Deliberately not
/// `get_param`, whose first branch resolves an environment variable — these
/// tests set that variable, and the question here is only what is on disk.
fn retired_key_on_disk() -> Option<Value> {
    Config::global()
        .all_values()
        .ok()
        .and_then(|m| m.get(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY).cloned())
}

/// **The gate DR-22 names.** Hand-edit `config.yaml`, restart, and the feature
/// is still on.
///
/// This single test fails the most plausible wrong implementation — the
/// compatibility reader that consults the new store *and* the old key — which
/// no amount of testing the happy path would catch. A reader that still
/// consults `config.yaml` has not closed the channel, it has added a second
/// one.
#[tokio::test]
#[serial_test::serial]
async fn a_hand_edited_config_yaml_cannot_turn_the_feature_off() {
    let _fixture = PrivacyToggleFixture::capture();
    reset_switch_storage();

    // Bring the install to the state a daemon reaches on its first start after
    // the upgrade: the migration has run and the store exists.
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "an install that never disabled the feature comes up ON"
    );

    // THE HAND EDIT. Exactly the file, the key and the value a user — or an
    // agent holding `developer__text_editor`, which DR-17 leaves able to write
    // this file — would put there.
    Config::global()
        .set(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            &Value::String("off".to_string()),
            false,
        )
        .unwrap();

    // THE RESTART. A fresh process comes up on the fail-safe default and then
    // loads; `load_privacy_tiers_from_config` is that load, and it is the only
    // thing a restart does to this value.
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(true);
    biorouter::privacy::load_privacy_tiers_from_config();

    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "a hand-written `config.yaml` key disabled the master switch across a restart — \
         the retired key must be IGNORED, not honoured for compatibility"
    );
}

/// The mirror. Disabling **through** the authenticated path survives a restart —
/// otherwise the gate above would be satisfied by a switch that simply cannot
/// be turned off, and the test above would pass while the feature was broken.
///
/// It also pins the two consequences of the move that the renderer depends on:
/// the value does **not** land in `config.yaml`, and `/config/read` and
/// `/config` still answer for the key — with the LIVE value, so Settings →
/// Privacy cannot show the user something false about the control they just
/// used.
#[tokio::test]
#[serial_test::serial]
async fn disabling_through_the_authenticated_path_survives_a_restart() {
    let _fixture = PrivacyToggleFixture::capture();
    reset_switch_storage();
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(biorouter::privacy::privacy_tiers_enabled());

    let _ok = upsert_config(
        HeaderMap::new(),
        Json(upsert(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            "off",
            Some(biorouter::privacy::PRIVACY_TIERS_DISABLE_PHRASE),
        )),
    )
    .await
    .expect("the confirmed flip is the one write this route allows");
    assert!(!biorouter::privacy::privacy_tiers_enabled());

    // The one door writes the store, and NOT `config.yaml`: leaving a copy
    // there would re-create the channel the move exists to close.
    assert_eq!(
        retired_key_on_disk(),
        None,
        "the master switch must not be written into config.yaml"
    );

    // Both config read paths still answer for the key, from the live value.
    // `ConfigContext` reads the whole map and `PrivacyPanel` reads the single
    // key; a blind reader would paint every badge as if the feature were on.
    match read_config(Json(ConfigKeyQuery {
        key: biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY.to_string(),
        is_secret: false,
    }))
    .await
    .expect("reading the master switch must not fail")
    .0
    {
        ConfigValueResponse::Value(v) => assert_eq!(v, Value::String("off".to_string())),
        ConfigValueResponse::MaskedValue(_) => {
            panic!("the master switch is not a secret and must not be masked")
        }
    }
    assert_eq!(
        read_all_config()
            .await
            .expect("reading the config map must not fail")
            .0
            .config
            .get(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY),
        Some(&Value::String("off".to_string())),
    );

    // THE RESTART.
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(true);
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        !biorouter::privacy::privacy_tiers_enabled(),
        "the user's own decision must survive a restart, or the switch is not a switch"
    );
}

/// The fourth state, and the one the gate above cannot reach: a store that
/// EXISTS but does not parse, with the retired key present and `off`.
///
/// ⚠ **Why the gate needs this beside it.** Every test above runs the migration
/// first, so the store is always *readable* by the time the hand-edited key is
/// read — which means one wrong implementation slips through all of them: the
/// compatibility reader written as `read_for(..).or_else(|| the key)`, whose
/// fallback arm is simply never taken. This is the state that takes it.
/// `exists_in` holds the migration shut (deliberately — one corrupt byte must
/// not make the retired key live again) while `read_for` answers `None`, so a
/// reader with a fallback reaches the key and honours it. Nothing recorded and
/// nothing readable both mean ON.
#[tokio::test]
#[serial_test::serial]
async fn an_unreadable_store_falls_back_to_enforcing_and_never_to_the_retired_key() {
    let _fixture = PrivacyToggleFixture::capture();
    reset_switch_storage();

    std::fs::write(config_dir().join("privacy-tiers.json"), "{ not json").unwrap();
    Config::global()
        .set(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            &Value::String("off".to_string()),
            false,
        )
        .unwrap();

    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(false);
    biorouter::privacy::load_privacy_tiers_from_config();

    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "an unreadable store fell through to the retired `config.yaml` key — a \
         record that does not parse must resolve to ON, never to whatever the \
         key says"
    );
    assert_eq!(
        retired_key_on_disk(),
        Some(Value::String("off".to_string())),
        "the migration ran against a store that already exists — one corrupt \
         byte must not re-open it"
    );
}

/// Step 2: the retired key is read **once**, at migration, and then ignored.
///
/// The first half is what users who set the key before this version are owed —
/// their answer is carried across rather than silently reset to ON. The second
/// half is why the migration is gated on the STORE's existence and not on the
/// key's: a migration that re-runs whenever the key reappears is the
/// compatibility reader wearing a different hat.
#[tokio::test]
#[serial_test::serial]
async fn the_retired_key_is_migrated_once_and_then_ignored() {
    let _fixture = PrivacyToggleFixture::capture();
    reset_switch_storage();

    // An install upgrading from a version where the key WAS the switch.
    Config::global()
        .set(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            &Value::String("off".to_string()),
            false,
        )
        .unwrap();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(true);
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        !biorouter::privacy::privacy_tiers_enabled(),
        "a value the user set before this version must be carried across, not reset"
    );
    assert!(
        config_dir().join("privacy-tiers.json").exists(),
        "the migration must leave the store behind — its existence is what stops \
         the migration running a second time"
    );
    assert_eq!(
        retired_key_on_disk(),
        None,
        "the migration must REMOVE the key, not leave it beside the store to drift"
    );

    // The user turns it back on through the one door…
    let _ok = upsert_config(
        HeaderMap::new(),
        Json(upsert(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            "on",
            Some(biorouter::privacy::PRIVACY_TIERS_DISABLE_PHRASE),
        )),
    )
    .await
    .expect("re-enabling goes through the same door");
    assert!(biorouter::privacy::privacy_tiers_enabled());

    // …and writing the key back by hand never migrates again.
    Config::global()
        .set(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            &Value::String("off".to_string()),
            false,
        )
        .unwrap();
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "the retired key was read a second time — 'once, at migration' means once"
    );
}
