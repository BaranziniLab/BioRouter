use std::path::{Path, PathBuf};

pub fn validate_kb_id(id: &str) -> Result<(), KbIdError> {
    if id.is_empty() {
        return Err(KbIdError::Empty);
    }
    if id.len() > 64 {
        return Err(KbIdError::TooLong);
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(KbIdError::InvalidChars);
    }
    if id.starts_with('-') || id.ends_with('-') || id.contains("--") {
        return Err(KbIdError::BadShape);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum KbIdError {
    #[error("kb-id is empty")]
    Empty,
    #[error("kb-id is longer than 64 characters")]
    TooLong,
    #[error("kb-id may only contain a-z, 0-9, and '-'")]
    InvalidChars,
    #[error("kb-id may not start/end with '-' or contain '--'")]
    BadShape,
}

/// `<config>/knowledge`, honouring `BIOROUTER_PATH_ROOT` via [`crate::paths`].
///
/// This used to hand-roll `choose_app_strategy` with an `io/biorouter/biorouter`
/// tuple — a *third* app-strategy in the codebase, and one that ignored the
/// sandbox override, so an isolated run read and wrote the user's global KBs.
/// It now shares the crate resolver (`Block/Block/biorouter`, matching
/// `biorouter::config::Paths`). The two tuples resolve identically on XDG and on
/// macOS; they differ only on Windows, where the old path was never the one the
/// rest of Biorouter used anyway.
pub fn knowledge_root() -> anyhow::Result<PathBuf> {
    Ok(crate::paths::in_config_dir("knowledge"))
}

pub fn kb_root(root: &Path, id: &str) -> PathBuf {
    root.join(id)
}

/// Returns `<knowledge-root>/.active-kb` — the file that persists the
/// **primary** knowledge base id (the write target for KB-less mutating calls).
///
/// The filename keeps its historical `.active-kb` spelling on purpose. The
/// merged model needs exactly one id, which is exactly what this file already
/// holds, so today's value *is* the primary and reading it is the entire
/// migration. It also keeps a lagging PATH-installed `biorouter` (see CLAUDE.md,
/// "Runtime CLI-vs-app drift") working: it reads a bare kb id whose meaning is
/// unchanged for it. Renaming the file, or writing anything structured into it,
/// would break that binary — `get_primary_persisted` performs no validation, so
/// it would happily join a JSON array into a filesystem path.
pub fn primary_kb_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".active-kb")
}

/// Returns `<knowledge-root>/.active-kb-sessions` — one file per session,
/// named `sha256(session_id)`, each holding that session's primary kb id.
/// Same naming rationale as [`primary_kb_path`].
pub fn primary_kb_sessions_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".active-kb-sessions")
}

pub fn hidden_kbs_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".hidden-kbs")
}

pub fn hidden_kb_sessions_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".hidden-kb-sessions")
}

pub fn kb_knowledge_dir(root: &Path, id: &str) -> PathBuf {
    kb_root(root, id).join("knowledge")
}

pub fn kb_raw_dir(root: &Path, id: &str) -> PathBuf {
    kb_root(root, id).join("raw")
}

pub fn kb_internal_dir(root: &Path, id: &str) -> PathBuf {
    kb_root(root, id).join(".biorouter-knowledge")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_good_ids() {
        for id in ["a", "ms-patient", "kb-01", "personal", "x1-y2-z3"] {
            assert!(validate_kb_id(id).is_ok(), "should accept {id}");
        }
    }

    #[test]
    fn rejects_bad_ids() {
        for (id, want) in [
            ("", KbIdError::Empty),
            ("ABC", KbIdError::InvalidChars),
            ("with space", KbIdError::InvalidChars),
            ("with/slash", KbIdError::InvalidChars),
            ("-leading", KbIdError::BadShape),
            ("trailing-", KbIdError::BadShape),
            ("dou--ble", KbIdError::BadShape),
            ("../escape", KbIdError::InvalidChars),
        ] {
            assert_eq!(validate_kb_id(id).unwrap_err(), want, "for {id}");
        }
    }

    #[test]
    fn path_helpers_compose() {
        let root = Path::new("/tmp/kb");
        assert_eq!(kb_root(root, "x"), Path::new("/tmp/kb/x"));
        assert_eq!(
            kb_knowledge_dir(root, "x"),
            Path::new("/tmp/kb/x/knowledge")
        );
        assert_eq!(kb_raw_dir(root, "x"), Path::new("/tmp/kb/x/raw"));
        assert_eq!(
            kb_internal_dir(root, "x"),
            Path::new("/tmp/kb/x/.biorouter-knowledge")
        );
    }
}
