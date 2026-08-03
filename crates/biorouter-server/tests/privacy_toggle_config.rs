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
    remove_config, upsert_config, ConfigKeyQuery, UpsertConfigQuery,
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
/// comes from the config FILE, and an environment variable cannot reach it.
#[tokio::test]
#[serial_test::serial]
async fn the_startup_load_reads_the_file_and_not_the_environment() {
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

    // The other test in this binary writes this key into the shared temp config,
    // and `#[serial]` orders the two but says nothing about which runs first.
    // Delete it so "absent" means absent whichever order they take.
    Config::global()
        .delete(biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY)
        .ok();
    biorouter_mcp::privacy_toggle::set_privacy_tiers_enabled(false);
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        biorouter::privacy::privacy_tiers_enabled(),
        "an absent key must resolve to ON, and the env var must not be consulted"
    );

    Config::global()
        .set(
            biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY,
            &Value::String("off".to_string()),
            false,
        )
        .unwrap();
    biorouter::privacy::load_privacy_tiers_from_config();
    assert!(
        !biorouter::privacy::privacy_tiers_enabled(),
        "the FILE is what the loader reads"
    );
}
