use crate::knowledge::{git::GitRepo, types::ChangeKind};
use anyhow::Result;
use chrono::Utc;
use std::path::Path;

pub fn append(
    kb_root: &Path,
    kind: ChangeKind,
    summary: &str,
    delta: Option<&str>,
    txn_branch: Option<&str>,
) -> Result<Option<String>> {
    let log_path = kb_root.join("log.md");
    let kind_str = match kind {
        ChangeKind::Ingest => "ingest",
        ChangeKind::Link => "link",
        ChangeKind::Flag => "flag",
        ChangeKind::Query => "query",
        ChangeKind::Lint => "lint",
        ChangeKind::Restore => "restore",
        ChangeKind::Manual => "manual",
    };
    let now = Utc::now().format("%Y-%m-%d");
    let line = match delta {
        Some(d) => format!("## [{now}] {kind_str} | {summary}\n\n{d}\n\n"),
        None => format!("## [{now}] {kind_str} | {summary}\n\n"),
    };
    let mut existing = if log_path.exists() {
        std::fs::read_to_string(&log_path)?
    } else {
        String::from("# Log\n\n")
    };
    existing.push_str(&line);
    let tmp = log_path.with_extension("md.tmp");
    std::fs::write(&tmp, existing)?;
    std::fs::rename(tmp, &log_path)?;

    let repo = GitRepo::open(kb_root)?;
    let sha = if let Some(_branch) = txn_branch {
        repo.commit_on_txn_in_progress(&format!("log: {kind_str} | {summary}"))?
    } else {
        repo.commit_all(kind, summary, delta)?
    };
    Ok(Some(sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        let kb = dir.path().join("k");
        (dir, kb)
    }

    #[test]
    fn append_writes_to_log_md() {
        let (_d, kb) = fresh();
        append(
            &kb,
            ChangeKind::Ingest,
            "first source",
            Some("+1 source"),
            None,
        )
        .unwrap();
        let body = std::fs::read_to_string(kb.join("log.md")).unwrap();
        assert!(body.contains("ingest | first source"));
        assert!(body.contains("+1 source"));
    }

    #[test]
    fn append_commits_to_git() {
        let (_d, kb) = fresh();
        let sha = append(&kb, ChangeKind::Manual, "test", None, None)
            .unwrap()
            .unwrap();
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        let log = repo.log(5).unwrap();
        assert_eq!(log[0].commit_sha, sha);
    }
}
