use std::{
    env::{self},
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use anyhow::{Context, Result};

use crate::config::Config;

pub struct SearchPaths {
    paths: Vec<PathBuf>,
}

impl SearchPaths {
    // NOTE: the standard user-tool dirs below (~/.local/bin, /usr/local/bin,
    // /opt/homebrew/bin, /opt/local/bin) are mirrored by
    // `standard_user_tool_dirs()` in
    // `crates/biorouter-mcp/src/developer/shell.rs` (issue #24) — that crate
    // cannot import this one (dependency direction is biorouter →
    // biorouter-mcp). Keep the two lists in sync when editing.
    pub fn builder() -> Self {
        let mut paths = Config::global()
            .get_biorouter_search_paths()
            .unwrap_or_default();

        paths.push("~/.local/bin".into());

        #[cfg(unix)]
        {
            paths.push("/usr/local/bin".into());
        }

        if cfg!(target_os = "macos") {
            paths.push("/opt/homebrew/bin".into());
            paths.push("/opt/local/bin".into());
        }

        Self {
            paths: paths
                .into_iter()
                .map(|s| PathBuf::from(shellexpand::tilde(&s).as_ref()))
                .collect(),
        }
    }

    /// Where a Node runtime is likely to live, for a child that is a Node
    /// script rather than a native binary.
    ///
    /// ⚠ This is not tidiness — without it the Codex provider fails outright.
    /// `codex` installs as `#!/usr/bin/env node`, so it cannot start at all
    /// unless `node` is on the child's PATH. A desktop app launched from Finder
    /// or the Dock inherits a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`),
    /// not the user's shell PATH, and the failure is silent and total:
    ///
    /// ```text
    /// env: node: No such file or directory     (exit 127, stdout never opened)
    /// ```
    ///
    /// which surfaced to the user as "app server closed its output".
    ///
    /// [`Self::builder`] already covers Homebrew and `/usr/local`. What it did
    /// not cover is a version manager, which is how a large share of developers
    /// install Node — nvm, fnm, Volta and asdf all put the runtime somewhere
    /// under `$HOME` that appears on PATH only because a shell profile put it
    /// there. A GUI child never runs that profile.
    ///
    /// Globs are deliberately avoided: nvm keeps one directory per installed
    /// version and picking between them is the shell's job, so `nvm/current` is
    /// taken when it exists and the rest are left alone.
    pub fn with_node_runtimes(mut self) -> Self {
        let Some(home) = dirs::home_dir() else {
            return self;
        };
        if cfg!(windows) {
            if let Some(appdata) = dirs::data_dir() {
                self.paths.push(appdata.join("Volta").join("bin"));
                self.paths.push(appdata.join("fnm"));
            }
            self.paths.push(home.join("scoop").join("shims"));
        } else {
            self.paths.push(home.join(".volta/bin"));
            self.paths.push(home.join(".nvm/current/bin"));
            self.paths.push(home.join(".fnm/aliases/default/bin"));
            self.paths.push(home.join(".asdf/shims"));
            self.paths.push(home.join(".local/share/fnm/aliases/default/bin"));
        }
        self
    }

    /// Put `dir` at the FRONT of the search path.
    ///
    /// Used for the directory a resolved coding-agent CLI was found in: an npm
    /// global install puts `node` and the CLI side by side, so the CLI's own
    /// directory is the single most reliable place to find the runtime it needs
    /// — more reliable than any list of guesses, because it is where this
    /// machine actually put it.
    pub fn with_leading_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.paths.insert(0, dir.into());
        self
    }

    pub fn with_npm(mut self) -> Self {
        if cfg!(windows) {
            if let Some(appdata) = dirs::data_dir() {
                self.paths.push(appdata.join("npm"));
            }
        } else if let Some(home) = dirs::home_dir() {
            self.paths.push(home.join(".npm-global/bin"));
        }
        self
    }

    pub fn path(self) -> Result<OsString> {
        env::join_paths(
            self.paths.into_iter().chain(
                env::var_os("PATH")
                    .as_ref()
                    .map(env::split_paths)
                    .into_iter()
                    .flatten(),
            ),
        )
        .map_err(Into::into)
    }

    pub fn resolve<N>(self, name: N) -> Result<PathBuf>
    where
        N: AsRef<OsStr>,
    {
        which::which_in_global(name.as_ref(), Some(self.path()?))?
            .next()
            .with_context(|| {
                format!(
                    "could not resolve command '{}': file does not exist",
                    name.as_ref().to_string_lossy()
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_preserves_existing_path() {
        let search_paths = SearchPaths::builder();
        let combined_path = search_paths.path().unwrap();

        if let Some(existing_path) = env::var_os("PATH") {
            let combined_str = combined_path.to_string_lossy();
            let existing_str = existing_path.to_string_lossy();

            assert!(combined_str.contains(&existing_str.to_string()));
        }
    }

    #[test]
    fn test_resolve_nonexistent_executable() {
        let search_paths = SearchPaths::builder();

        let result = search_paths.resolve("nonexistent_executable_12345_abcdef");

        assert!(
            result.is_err(),
            "Resolving nonexistent executable should return an error"
        );
    }

    #[test]
    fn test_resolve_common_executable() {
        let search_paths = SearchPaths::builder();

        #[cfg(unix)]
        let test_executable = "sh";

        #[cfg(windows)]
        let test_executable = "cmd";

        search_paths
            .resolve(test_executable)
            .expect("should resolve sh (or cmd on Windows)");
    }

    /// The coding-agent providers' PATH must be able to find a Node runtime.
    ///
    /// ⚠ This is a startup precondition, not a nicety. `codex` installs as
    /// `#!/usr/bin/env node`; with no `node` on PATH it exits 127 without ever
    /// opening stdout, and the user is told only "app server closed its output".
    #[test]
    fn a_coding_agent_path_carries_the_cli_directory_first() {
        let dir = std::path::PathBuf::from("/somewhere/node_modules/.bin");
        let path = SearchPaths::builder()
            .with_npm()
            .with_node_runtimes()
            .with_leading_dir(dir.clone())
            .path()
            .expect("joinable");
        let first = env::split_paths(&path).next().expect("at least one entry");
        assert_eq!(
            first, dir,
            "the resolved CLI's own directory must come FIRST: an npm global              install puts `node` beside the CLI, which is the one place this              machine is known to have put it"
        );
    }

    /// The version managers a GUI child never inherits, because it does not run
    /// a shell profile. Asserted by SHAPE rather than by existence — a CI runner
    /// has none of them installed, and a test that skipped when they were absent
    /// would pass everywhere and prove nothing.
    #[test]
    fn a_coding_agent_path_offers_the_node_version_managers() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let path = SearchPaths::builder()
            .with_npm()
            .with_node_runtimes()
            .path()
            .expect("joinable");
        let entries: Vec<_> = env::split_paths(&path).collect();

        #[cfg(not(windows))]
        let wanted: Vec<PathBuf> = vec![
            home.join(".volta/bin"),
            home.join(".nvm/current/bin"),
            home.join(".asdf/shims"),
        ];
        #[cfg(windows)]
        let wanted: Vec<PathBuf> = vec![home.join("scoop").join("shims")];

        for w in wanted {
            assert!(
                entries.contains(&w),
                "{} missing from the coding-agent PATH: {entries:?}",
                w.display()
            );
        }
    }

    /// The pre-existing roots must survive. Issue #24 was a truncated PATH
    /// breaking every Homebrew binary; widening must add, never replace.
    #[test]
    fn adding_node_runtimes_keeps_the_standard_tool_dirs() {
        let plain: Vec<PathBuf> = env::split_paths(&SearchPaths::builder().path().unwrap()).collect();
        let widened: Vec<PathBuf> =
            env::split_paths(&SearchPaths::builder().with_node_runtimes().path().unwrap()).collect();
        for entry in plain {
            assert!(
                widened.contains(&entry),
                "{} was dropped when the node runtimes were added",
                entry.display()
            );
        }
    }
}
