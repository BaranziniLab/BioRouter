//! Sandbox the `biorouter` **lib** test binary's config/data root before any
//! test runs.
//!
//! The integration binary `tests/agent.rs` has had this since issue #54; the
//! lib binary never did, and BR-71 made that gap load-bearing. Several tests
//! here reach `AgentManager::instance()` — `workspace_list`'s handler (Tasks
//! 12/14/15/17) and, since Task 33, `run_complete_subagent_task`'s child
//! registration. `instance()` resolves `Paths::data_dir()` and
//! `SessionManager::instance()`, and its first initialization runs
//! `run_first_run_init`, which seeds the built-in skills and installs the Soul
//! KB plus a 3 AM schedule. Unsandboxed, a plain `cargo test -p biorouter --lib`
//! therefore writes into the developer's real `~/.config/biorouter`.
//!
//! Documenting "run it under `BIOROUTER_PATH_ROOT`" is not a fix: the damage
//! happens on the DEFAULT invocation, which is what a developer types and what
//! an editor's test runner emits. This makes the sandbox the default and lets
//! the gate keep choosing its own root.

/// Run before `main`, because the placement is the whole point.
///
/// `Config::global()`, `SessionManager::instance()` and `AGENT_MANAGER` are all
/// one-shot cells that resolve their path the first time anything touches them,
/// and the tests run in parallel — so a guard installed inside whichever test
/// module "owns" the hazard fixes nothing whenever another test got there
/// first. Running before any test is the only placement that cannot lose that
/// race (the same reasoning as `tests/agent.rs`).
///
/// An outer `BIOROUTER_PATH_ROOT` wins: the Task 33 gate exports its own
/// `mktemp -d` root, and a harness that wants to inspect what a run wrote must
/// be able to choose where it lands.
#[ctor::ctor]
fn sandbox_config_root_for_the_lib_test_binary() {
    if std::env::var_os("BIOROUTER_PATH_ROOT").is_some() {
        return;
    }
    let root = tempfile::TempDir::new().expect("scratch config root for the lib test binary");
    std::env::set_var("BIOROUTER_PATH_ROOT", root.path());
    // Leaked deliberately: a `static` is never dropped, which is exactly the
    // lifetime the sandbox needs — it must outlive the last test in the binary.
    static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let _ = ROOT.set(root);
}

#[cfg(test)]
mod tests {
    /// Nothing in this binary may resolve to the developer's live configuration.
    ///
    /// Asserting on `Config::global()` rather than on the environment is what
    /// makes this meaningful: it is the resolved path a write actually follows,
    /// and it is frozen at first use. A test that only checked the env var would
    /// still pass if something had reached `Config::global()` before the ctor.
    #[test]
    fn the_lib_test_binary_config_root_is_sandboxed() {
        let root = std::env::var("BIOROUTER_PATH_ROOT")
            .expect("BIOROUTER_PATH_ROOT must be sandboxed before any test in this binary runs");
        let path = crate::config::Config::global().path();
        assert!(
            path.starts_with(&root),
            "Config::global() resolved to {path}, outside the sandbox at {root}. \
             Something reached Config::global() before the sandbox was installed, so \
             config writes from this binary land in the developer's real config."
        );
    }
}
