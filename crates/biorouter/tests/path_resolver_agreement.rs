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

/// Runs `f` with `BIOROUTER_PATH_ROOT` set, restoring the prior value after.
/// Serialized because it mutates process-wide env.
fn with_path_root<T>(root: Option<&str>, f: impl FnOnce() -> T) -> T {
    let prev = std::env::var("BIOROUTER_PATH_ROOT").ok();
    match root {
        Some(r) => std::env::set_var("BIOROUTER_PATH_ROOT", r),
        None => std::env::remove_var("BIOROUTER_PATH_ROOT"),
    }
    let out = f();
    match prev {
        Some(p) => std::env::set_var("BIOROUTER_PATH_ROOT", p),
        None => std::env::remove_var("BIOROUTER_PATH_ROOT"),
    }
    out
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
