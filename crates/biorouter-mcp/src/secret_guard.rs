//! Central secret-redaction boundary (BR-23).
//!
//! Historically the `.biorouterignore` deny list lived only inside the Developer
//! MCP server (`developer::rmcp_developer`), so any *other* extension — compute,
//! files, a third-party MCP server, or a different shell wrapper — that read a
//! `.env`/`secrets.*`/private-key file bypassed it entirely. `SecretGuard`
//! extracts that logic into one shared type so it can be enforced at the
//! extension-manager dispatch boundary — the single choke point every tool call
//! flows through — as well as inside the Developer server itself.
//!
//! Two behavioural improvements over the old Developer-local matcher:
//!   * The built-in secret patterns are an always-on *floor* (they apply even
//!     when a project ships its own `.biorouterignore`, which previously silently
//!     dropped them). A user can still opt back into a specific file with a
//!     gitignore negation (`!path`) because the floor is added *before* the
//!     user's patterns and gitignore matching is last-match-wins.
//!   * The default deny set is widened beyond `.env`/`secrets.*` to cover private
//!     keys (`*.pem`, `id_rsa`, `id_ed25519`, …) and cloud-credential files
//!     (`.aws/credentials`, `*.p12`, `*.pfx`).

use etcetera::AppStrategy;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Built-in secret/credential deny patterns applied as an always-on floor.
///
/// Kept deliberately tight — every entry names a file that is a secret by
/// convention — so the exists-gated dispatch scan does not block legitimate
/// reads of ordinary config files.
pub const DEFAULT_SECRET_PATTERNS: &[&str] = &[
    "**/.env",
    "**/.env.*",
    "**/secrets.*",
    "**/*.pem",
    "**/id_rsa",
    "**/id_dsa",
    "**/id_ecdsa",
    "**/id_ed25519",
    "**/*.p12",
    "**/*.pfx",
    "**/.aws/credentials",
];

/// Object keys whose string values are treated as file paths and scanned in
/// full (every whitespace token). Tokens under any other key are only scanned
/// when they contain a path separator, so a `.env` mentioned in prose (e.g. a
/// `content`/`message` field) does not trip the boundary.
fn key_is_pathlike(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("path")
        || k.contains("file")
        || k.contains("dir")
        || matches!(
            k.as_str(),
            "command"
                | "cmd"
                | "script"
                | "target"
                | "output"
                | "out"
                | "input"
                | "src"
                | "source"
                | "dest"
                | "destination"
                | "location"
                | "uri"
                | "url"
                | "folder"
        )
}

/// A reusable secret/credential access guard rooted at a working directory.
#[derive(Clone)]
pub struct SecretGuard {
    ignore: Gitignore,
    root: PathBuf,
}

impl SecretGuard {
    /// Build a guard rooted at `cwd`, combining the always-on secret floor with
    /// any project-local (`<cwd>/.biorouterignore`) and global
    /// (`<config>/.biorouterignore`) ignore files. User patterns are layered
    /// *after* the floor so a `!path` negation can un-ignore a specific file the
    /// user explicitly wants read.
    pub fn for_dir(cwd: &Path) -> Self {
        let mut builder = GitignoreBuilder::new(cwd);

        for pat in DEFAULT_SECRET_PATTERNS {
            let _ = builder.add_line(None, pat);
        }

        let global_ignore_path = etcetera::choose_app_strategy(crate::APP_STRATEGY.clone())
            .map(|strategy| strategy.config_dir().join(".biorouterignore"))
            .ok();
        if let Some(p) = global_ignore_path.as_ref() {
            if p.is_file() {
                let _ = builder.add(p);
            }
        }

        let local_ignore_path = cwd.join(".biorouterignore");
        if local_ignore_path.is_file() {
            let _ = builder.add(&local_ignore_path);
        }

        // Degrade to an empty matcher on a malformed ignore file rather than
        // panicking on the dispatch path.
        let ignore = builder.build().unwrap_or_else(|_| Gitignore::empty());
        Self {
            ignore,
            root: cwd.to_path_buf(),
        }
    }

    /// The underlying gitignore matcher, for callers (e.g. the code analyzer)
    /// that traverse trees and need the raw matcher.
    pub fn gitignore(&self) -> &Gitignore {
        &self.ignore
    }

    /// True when `path` matches a deny pattern. IO-free (treats the path as a
    /// file), so it is cheap to call per-token before touching the filesystem.
    pub fn is_denied(&self, path: &Path) -> bool {
        self.ignore.matched(path, false).is_ignore()
    }

    /// Scan a tool call's arguments for a reference to a denied (secret) file
    /// that *exists* on disk, resolved against the guard's root. Returns the
    /// offending display path, or `None`.
    ///
    /// Conservative by design:
    ///   * a candidate is only reported when the resolved path exists, so a
    ///     benign mention of `.env` that does not name a real file never trips
    ///     the boundary, and creating a *new* secret file (not yet on disk) is
    ///     not blocked;
    ///   * a bare token (no path separator) is only considered under a path-like
    ///     key, so `.env` appearing inside a `content`/`message` field is
    ///     ignored even when such a file happens to exist.
    pub fn find_denied_path(&self, arguments: &Map<String, Value>) -> Option<String> {
        let mut found = None;
        for (key, value) in arguments {
            self.walk_value(Some(key), value, &mut found);
            if found.is_some() {
                break;
            }
        }
        found
    }

    fn walk_value(&self, key: Option<&str>, value: &Value, found: &mut Option<String>) {
        if found.is_some() {
            return;
        }
        match value {
            Value::String(s) => self.scan_string(key, s, found),
            Value::Array(items) => {
                for item in items {
                    self.walk_value(key, item, found);
                    if found.is_some() {
                        return;
                    }
                }
            }
            Value::Object(map) => {
                for (k, v) in map {
                    self.walk_value(Some(k), v, found);
                    if found.is_some() {
                        return;
                    }
                }
            }
            _ => {}
        }
    }

    fn scan_string(&self, key: Option<&str>, s: &str, found: &mut Option<String>) {
        let pathlike_key = key.map(key_is_pathlike).unwrap_or(false);
        let trimmed = s.trim();

        // Whole-string candidate (covers a plain `{"path": ".env"}` argument).
        if (pathlike_key || has_separator(trimmed)) && self.candidate_is_denied(trimmed) {
            *found = Some(trimmed.to_string());
            return;
        }

        // Token candidates (covers shell command lines and multi-path values).
        for token in trimmed.split_whitespace() {
            let tok = token.trim_matches(|c| c == '"' || c == '\'');
            if tok.is_empty() || tok.starts_with('-') {
                continue;
            }
            if (pathlike_key || has_separator(tok)) && self.candidate_is_denied(tok) {
                *found = Some(tok.to_string());
                return;
            }
        }
    }

    /// Pattern-match first (IO-free), then confirm the file exists. Ordering
    /// keeps the common case (large non-path arguments) from issuing a `stat`
    /// per token.
    fn candidate_is_denied(&self, candidate: &str) -> bool {
        if candidate.is_empty() {
            return false;
        }
        let path = Path::new(candidate);
        // `join` replaces the base when `path` is absolute, so this handles both
        // relative and absolute candidates.
        let resolved = self.root.join(path);
        if !(self.is_denied(&resolved) || self.is_denied(path)) {
            return false;
        }
        resolved.exists() || path.exists()
    }
}

fn has_separator(s: &str) -> bool {
    s.contains('/') || s.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn guard_at(dir: &Path) -> SecretGuard {
        SecretGuard::for_dir(dir)
    }

    #[test]
    fn widened_default_patterns_match() {
        let dir = tempdir().unwrap();
        let g = guard_at(dir.path());
        for name in [
            ".env",
            ".env.local",
            "secrets.yaml",
            "server.pem",
            "id_rsa",
            "id_ed25519",
            "keystore.p12",
            "cert.pfx",
        ] {
            assert!(g.is_denied(Path::new(name)), "expected {name} to be denied");
        }
        assert!(g.is_denied(Path::new(".aws/credentials")));
        assert!(!g.is_denied(Path::new("normal.txt")));
        assert!(!g.is_denied(Path::new("data.csv")));
    }

    #[test]
    fn find_denied_requires_existing_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        let g = guard_at(dir.path());

        // Existing secret referenced by a path-like key -> blocked.
        let args = json!({ "path": ".env" });
        assert_eq!(
            g.find_denied_path(args.as_object().unwrap()),
            Some(".env".to_string())
        );

        // Non-existent secret -> not blocked (creating a new .env is allowed).
        let args = json!({ "path": "config/.env" });
        assert_eq!(g.find_denied_path(args.as_object().unwrap()), None);
    }

    #[test]
    fn shell_command_token_is_scanned() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        let g = guard_at(dir.path());
        let args = json!({ "command": "cat .env" });
        assert_eq!(
            g.find_denied_path(args.as_object().unwrap()),
            Some(".env".to_string())
        );
    }

    #[test]
    fn prose_mention_under_non_path_key_is_not_blocked() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        let g = guard_at(dir.path());
        // A bare `.env` (no separator) under a non-path key must not trip the
        // boundary even though the file exists.
        let args = json!({ "content": "remember to copy .env.example to .env" });
        assert_eq!(g.find_denied_path(args.as_object().unwrap()), None);
    }

    #[test]
    fn separator_path_under_any_key_is_scanned() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/.env"), "SECRET=1").unwrap();
        let g = guard_at(dir.path());
        // Unknown key, but the token carries a path separator + resolves to an
        // existing secret -> blocked.
        let args = json!({ "resource": "sub/.env" });
        assert_eq!(
            g.find_denied_path(args.as_object().unwrap()),
            Some("sub/.env".to_string())
        );
    }

    #[test]
    fn benign_arguments_pass() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("data.csv"), "a,b").unwrap();
        let g = guard_at(dir.path());
        let args = json!({ "path": "data.csv", "command": "wc -l data.csv" });
        assert_eq!(g.find_denied_path(args.as_object().unwrap()), None);
    }

    #[test]
    fn secret_floor_survives_custom_biorouterignore() {
        let dir = tempdir().unwrap();
        // A project that ignores build artifacts must not lose .env protection.
        fs::write(dir.path().join(".biorouterignore"), "target/\n*.log\n").unwrap();
        let g = guard_at(dir.path());
        assert!(g.is_denied(Path::new(".env")));
        assert!(g.is_denied(Path::new("secrets.yaml")));
        assert!(g.is_denied(Path::new("build.log")));
        assert!(!g.is_denied(Path::new("normal.txt")));
    }

    #[test]
    fn negation_can_reopen_a_specific_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".biorouterignore"), "!secrets.public\n").unwrap();
        let g = guard_at(dir.path());
        // The negation reopens this one file...
        assert!(!g.is_denied(Path::new("secrets.public")));
        // ...but the rest of the floor still stands.
        assert!(g.is_denied(Path::new(".env")));
        assert!(g.is_denied(Path::new("secrets.yaml")));
    }
}
