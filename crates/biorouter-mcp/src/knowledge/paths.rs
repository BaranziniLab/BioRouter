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

pub fn knowledge_root() -> anyhow::Result<PathBuf> {
    use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
    let strategy = choose_app_strategy(AppStrategyArgs {
        top_level_domain: "io".to_string(),
        author: "biorouter".to_string(),
        app_name: "biorouter".to_string(),
    })?;
    Ok(strategy.config_dir().join("knowledge"))
}

pub fn kb_root(root: &Path, id: &str) -> PathBuf {
    root.join(id)
}

/// Returns `<knowledge-root>/.active-kb` — the file that persists the
/// currently-active KB id across MCP-server processes.
pub fn active_kb_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".active-kb")
}

pub fn active_kb_sessions_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".active-kb-sessions")
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
