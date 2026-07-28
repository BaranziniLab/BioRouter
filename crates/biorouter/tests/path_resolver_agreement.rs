//! `biorouter-mcp` cannot depend on `biorouter` (circular), so it carries its own
//! copy of the config-dir resolver. This test — in the one crate that can see
//! both — pins the two together.
//!
//! It exists because three hand-rolled `choose_app_strategy(...)` calls in
//! `biorouter-mcp` silently ignored `BIOROUTER_PATH_ROOT`, so an isolated run
//! (a test drive, a worktree, a per-app jail) wrote its drafted apps into, and
//! read knowledge bases out of, the user's **global** store. If someone adds a
//! fourth hand-rolled resolver, this fails.

use biorouter::config::paths::Paths;
use serial_test::serial;
use std::path::PathBuf;

/// Runs `f` with `BIOROUTER_PATH_ROOT` pinned to `root`, restoring the prior
/// value after.
///
/// The `env_lock` guard is what makes this safe, and it does two things a bare
/// `set_var`/`remove_var` pair cannot. It restores the previous value from
/// `Drop`, so an assertion failure inside `f` cannot leak this file's scratch
/// root into the rest of the process; and it holds the same process-wide mutex
/// every other `BIOROUTER_PATH_ROOT` test in the workspace takes, which
/// `#[serial]` does not — `#[serial]` only orders tests that are themselves
/// annotated, and this variable is read by `Paths::*` on every call from
/// anywhere.
fn with_path_root<T>(root: Option<&str>, f: impl FnOnce() -> T) -> T {
    let _guard = env_lock::lock_env([("BIOROUTER_PATH_ROOT", root)]);
    f()
}

#[test]
#[serial]
fn mcp_config_dir_matches_the_authoritative_resolver() {
    for root in [None, Some("/tmp/br-path-agreement")] {
        with_path_root(root, || {
            assert_eq!(
                biorouter_mcp::paths::config_dir(),
                Paths::config_dir(),
                "biorouter-mcp::paths::config_dir() drifted from biorouter::config::Paths \
                 (BIOROUTER_PATH_ROOT={root:?})"
            );
        });
    }
}

#[test]
#[serial]
fn agent_drafter_store_honours_the_sandbox_root() {
    with_path_root(Some("/tmp/br-path-agreement"), || {
        assert_eq!(
            biorouter_mcp::agent_drafter::default_root(),
            PathBuf::from("/tmp/br-path-agreement/config/agent_drafter"),
            "drafted apps must land inside the sandbox, not the user's global store"
        );
    });
}

#[test]
#[serial]
fn knowledge_store_honours_the_sandbox_root() {
    with_path_root(Some("/tmp/br-path-agreement"), || {
        assert_eq!(
            biorouter_mcp::knowledge::paths::knowledge_root().expect("knowledge root"),
            PathBuf::from("/tmp/br-path-agreement/config/knowledge"),
            "knowledge bases must be read from the sandbox, not the user's global store"
        );
    });
}

/// `with_path_root` must put the previous value back even when the closure
/// panics. An assertion failure inside any test above otherwise leaks this
/// file's scratch root into every other test running in the process — the exact
/// failure that made a full `cargo test` run red (a test resolved into a
/// `TempDir` that had already been dropped).
///
/// The panic below is caught, so the stderr backtrace it prints is expected.
#[test]
#[serial]
fn with_path_root_restores_the_previous_value_on_panic() {
    let before = std::env::var("BIOROUTER_PATH_ROOT").ok();

    let outcome = std::panic::catch_unwind(|| {
        with_path_root(Some("/tmp/br-path-agreement-leak"), || {
            panic!("simulated assertion failure inside the guarded region");
        })
    });

    assert!(outcome.is_err(), "the closure's panic must propagate");
    assert_eq!(
        std::env::var("BIOROUTER_PATH_ROOT").ok(),
        before,
        "BIOROUTER_PATH_ROOT leaked out of with_path_root when the test body panicked"
    );
}

/// The three resolvers must sit under one config root — not, as before, under
/// two different app-strategy tuples (`Block/Block/biorouter` vs `io/biorouter/biorouter`).
#[test]
#[serial]
fn all_mcp_stores_share_one_config_root() {
    with_path_root(None, || {
        let cfg = Paths::config_dir();
        assert!(biorouter_mcp::agent_drafter::default_root().starts_with(&cfg));
        assert!(biorouter_mcp::knowledge::paths::knowledge_root()
            .expect("knowledge root")
            .starts_with(&cfg));
    });
}
