//! Points a test binary's Biorouter data/config/state dirs at a throwaway
//! directory, before the first line of test code runs.
//!
//! Without this, `AppState::new()` — and every test that goes through it —
//! opens the **developer's real** `sessions.db`. Tests then create, rename and
//! delete rows in the same database that holds the user's actual chats. Nothing
//! has been lost so far only because the tests happen to clean up after
//! themselves; an interrupted or buggy test is one mistake away from touching
//! real data.
//!
//! The mechanism already exists — `biorouter::config::Paths::get_dir` honours
//! `BIOROUTER_PATH_ROOT` — but it is read through a `LazyLock`
//! (`session_manager::SESSION_STORAGE`) and a `OnceCell`
//! (`AgentManager::instance`), so a test that sets it *from inside* a `#[test]`
//! fn only wins if it happens to be the first test to touch the singleton. That
//! is why it has to be a `#[ctor]`: constructors run before `main`, before the
//! harness spawns a thread, so the singletons cannot already be pinned to the
//! real database when the override lands.
//!
//! Isolation is therefore per test *binary*, which is exactly the granularity
//! the flake needed: `#[serial]` already serialises tests within one binary, but
//! `cargo test` runs several binaries in parallel against one file, and SQLite
//! answered that with `(code: 5) database is locked` about one run in five.
//!
//! An externally supplied `BIOROUTER_PATH_ROOT` is left alone, so a harness that
//! already sandboxes the process keeps its own root.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set only when *we* created the sandbox, so the destructor can never delete a
/// root that was handed to us from outside.
static OWNED: AtomicBool = AtomicBool::new(false);

/// Per-process sandbox root. Derived from the pid so the destructor can
/// recompute it without shared state, and so parallel test binaries never
/// collide.
fn sandbox_root() -> PathBuf {
    std::env::temp_dir().join(format!("biorouter-test-root-{}", std::process::id()))
}

#[ctor::ctor]
fn redirect_biorouter_dirs_into_a_sandbox() {
    if std::env::var("BIOROUTER_PATH_ROOT").is_ok_and(|v| !v.trim().is_empty()) {
        return;
    }
    let root = sandbox_root();
    if std::fs::create_dir_all(&root).is_err() {
        return;
    }
    OWNED.store(true, Ordering::SeqCst);
    std::env::set_var("BIOROUTER_PATH_ROOT", &root);
}

#[ctor::dtor]
fn remove_the_sandbox() {
    if OWNED.load(Ordering::SeqCst) {
        let _ = std::fs::remove_dir_all(sandbox_root());
    }
}

#[cfg(test)]
mod tests {
    use biorouter::config::paths::Paths;

    /// The point of the whole module: the session database a test opens must not
    /// be the developer's. `Paths::data_dir()` is what
    /// `SessionManager::instance()` resolves `sessions.db` under.
    ///
    /// ⚠ The home directory is read under **both** names, and the fallback is
    /// not defensive padding. `HOME` is a POSIX variable; Windows calls it
    /// `USERPROFILE` and does not define `HOME` outside a Git Bash session, so
    /// `var("HOME").expect("HOME")` panicked this test on any runner that
    /// happened not to have one. It is the only unguarded `HOME` read left in
    /// the tree: `security::session_store` and `security::global_memory` both
    /// already spell it `HOME` or `USERPROFILE`, and this now matches them.
    /// Falling back to an empty string keeps the assertion meaningful when
    /// neither is set, because `starts_with("")` is true for every path, so a
    /// missing home makes the check STRICTER rather than vacuous.
    #[test]
    fn the_session_database_is_not_the_developers() {
        let data_dir = Paths::data_dir();
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        assert!(
            !data_dir.starts_with(&home) || data_dir.starts_with(std::env::temp_dir()),
            "tests resolved the real data dir {}",
            data_dir.display()
        );
        assert!(
            data_dir.starts_with(super::sandbox_root()),
            "expected the per-process sandbox, got {}",
            data_dir.display()
        );
    }
}
