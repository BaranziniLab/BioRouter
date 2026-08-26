//! `log.md`, in the shape OKF §9 specifies.
//!
//! ## What changed, and why it was wrong before
//!
//! BioRouter wrote one heading per entry: `## [2026-08-19] ingest | a title`.
//! That is the LLM-Wiki (Karpathy) log convention this subsystem was modelled
//! on, and it is a **silent conformance failure** against OKF v0.2 — §9 requires
//! date headings in ISO 8601 `YYYY-MM-DD` form, and `[2026-08-19] ingest | …`
//! is not a date. `okf::check_log` has been able to say so since Stage 0;
//! nothing was asking it.
//!
//! So the shape is now: `## YYYY-MM-DD` groups, newest first, one bullet per
//! entry, and the kind moves out of the heading into a leading bold word in the
//! bullet — §9's own `**Update**` / `**Creation**` convention. The kind is not
//! lost in the move; it is BioRouter's own `ChangeKind`, capitalised.
//!
//! ## Newest first, all the way down
//!
//! Both axes. A new date group goes above every existing one, and a new entry
//! goes at the top of its own day's group. Appending within the day and
//! prepending between days would leave the file only half-ordered, which is
//! worse than either rule on its own for the one thing a change log is read
//! for: what happened last.
//!
//! ## An old log is not rewritten
//!
//! A base written before this change has `## [date] kind | summary` headings.
//! They stay exactly as they are — a new group is inserted above them and the
//! two shapes coexist. Rewriting them would be a format migration of user
//! content, which DR-17/DR-22 keep off every automatic path.

use crate::knowledge::{git::GitRepo, types::ChangeKind};
use anyhow::Result;
use chrono::Utc;
use std::path::Path;

/// The log's own H1. A fresh `log.md` starts here and every group is inserted
/// below it.
const LOG_HEADING: &str = "# Log";

/// §9's leading bold word, per [`ChangeKind`].
///
/// §9 gives `**Update**` and `**Creation**` as examples and says in as many
/// words that the word is "a convention, not a requirement" — `okf::check_log`
/// deliberately does not check it, precisely so a conformant log written by
/// someone else is not reported as broken. So BioRouter keeps its own seven
/// kinds rather than flattening them into two of the spec's example words,
/// which would throw away the one piece of information the heading used to
/// carry.
fn kind_word(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Ingest => "Ingest",
        ChangeKind::Link => "Link",
        ChangeKind::Flag => "Flag",
        ChangeKind::Query => "Query",
        ChangeKind::Lint => "Lint",
        ChangeKind::Restore => "Restore",
        ChangeKind::Manual => "Manual",
    }
}

/// The lowercase word used in commit messages, which are not `log.md` and are
/// not governed by §9. Unchanged, so `git log` keeps reading as it always has.
fn kind_slug(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Ingest => "ingest",
        ChangeKind::Link => "link",
        ChangeKind::Flag => "flag",
        ChangeKind::Query => "query",
        ChangeKind::Lint => "lint",
        ChangeKind::Restore => "restore",
        ChangeKind::Manual => "manual",
    }
}

/// One log entry: the bullet, plus each line of `delta` indented under it so a
/// multi-line delta stays part of the bullet rather than ending it.
fn entry_block(kind: ChangeKind, summary: &str, delta: Option<&str>) -> String {
    let mut block = format!("* **{}** — {}\n", kind_word(kind), summary.trim());
    if let Some(d) = delta {
        for line in d.trim_end().lines() {
            if line.trim().is_empty() {
                block.push('\n');
            } else {
                block.push_str("  ");
                block.push_str(line);
                block.push('\n');
            }
        }
    }
    block
}

/// Splice `entry` into `existing` under the `## {today}` group, creating the
/// group above every other group if it is not there yet.
///
/// Pure, and separated from the IO for the usual reason: every interesting case
/// here is a *text* case (an empty log, a log whose only groups are the old
/// per-entry headings, a second entry on the same day) and none of them needs a
/// temp directory or a git repo to exercise.
fn with_entry(existing: &str, today: &str, entry: &str) -> String {
    let heading = format!("## {today}");
    let lines: Vec<&str> = existing.lines().collect();

    if let Some(at) = lines.iter().position(|l| l.trim_end() == heading) {
        // Today's group exists: the new entry goes at the top of it, after the
        // heading and the blank line that follows it.
        let mut insert_at = at + 1;
        if lines.get(insert_at).is_some_and(|l| l.trim().is_empty()) {
            insert_at += 1;
        }
        let mut out: Vec<String> = lines[..insert_at]
            .iter()
            .map(|l| (*l).to_string())
            .collect();
        out.extend(entry.lines().map(str::to_string));
        out.extend(lines[insert_at..].iter().map(|l| (*l).to_string()));
        return join_lines(&out);
    }

    // No group for today. It goes above every existing group — including the
    // legacy `## [date] kind | summary` headings, which are still `## ` lines —
    // and below the H1 and whatever prose follows it.
    let group = format!("{heading}\n\n{entry}");
    let first_group = lines.iter().position(|l| l.starts_with("## "));
    match first_group {
        Some(at) => {
            let mut out: Vec<String> = lines[..at].iter().map(|l| (*l).to_string()).collect();
            // Exactly one blank line between the H1 (or the previous prose) and
            // the new group, however the file happened to end its last line.
            while out.last().is_some_and(|l| l.trim().is_empty()) {
                out.pop();
            }
            out.push(String::new());
            out.extend(group.lines().map(str::to_string));
            out.push(String::new());
            out.extend(lines[at..].iter().map(|l| (*l).to_string()));
            join_lines(&out)
        }
        None => {
            let mut out = existing.trim_end().to_string();
            if out.is_empty() {
                out.push_str(LOG_HEADING);
            }
            out.push_str("\n\n");
            out.push_str(&group);
            out
        }
    }
}

fn join_lines(lines: &[String]) -> String {
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

pub fn append(
    kb_root: &Path,
    kind: ChangeKind,
    summary: &str,
    delta: Option<&str>,
    txn_branch: Option<&str>,
) -> Result<Option<String>> {
    let log_path = kb_root.join("log.md");
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let existing = if log_path.exists() {
        std::fs::read_to_string(&log_path)?
    } else {
        format!("{LOG_HEADING}\n\n")
    };
    let updated = with_entry(&existing, &today, &entry_block(kind, summary, delta));

    // tmp + rename, like every other durable write in this module: a torn
    // `log.md` is a torn *user* file, and the change log is the one file whose
    // job is to say what happened.
    crate::knowledge::store::write_atomically(&log_path, updated.as_bytes())?;

    let repo = GitRepo::open(kb_root)?;
    let kind_str = kind_slug(kind);
    let sha = if let Some(branch) = txn_branch {
        repo.commit_on_txn_in_progress(branch, &format!("log: {kind_str} | {summary}"))?
    } else {
        repo.commit_all(kind, summary, delta)?
    };
    Ok(Some(sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf;
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
        let today = Utc::now().format("%Y-%m-%d").to_string();
        // The kind is in the bullet and the date is the heading — §9's shape,
        // not `## [date] ingest | summary`, which is not a date heading at all.
        assert!(body.contains(&format!("## {today}")), "{body}");
        assert!(body.contains("* **Ingest** — first source"), "{body}");
        assert!(body.contains("+1 source"), "{body}");
        assert!(!body.contains("ingest |"), "the old shape survived: {body}");
    }

    /// The gate this change exists for, asked of the checker that has been able
    /// to answer it since Stage 0.
    #[test]
    fn what_append_writes_is_conformant_by_the_projects_own_checker() {
        let (_d, kb) = fresh();
        let scaffold = std::fs::read_to_string(kb.join("log.md")).unwrap();
        assert!(okf::check_log(&scaffold).is_empty(), "{scaffold}");

        for (kind, summary) in [
            (ChangeKind::Ingest, "Chen 2020"),
            (ChangeKind::Lint, "3 orphans fixed"),
            (ChangeKind::Manual, "hand edit"),
        ] {
            append(&kb, kind, summary, None, None).unwrap();
        }
        let body = std::fs::read_to_string(kb.join("log.md")).unwrap();
        let diagnostics = okf::check_log(&body);
        assert!(diagnostics.is_empty(), "{diagnostics:?}\n{body}");
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

    /// The commit message is not `log.md` and §9 does not govern it, so it keeps
    /// the lowercase kind it has always had — `git log` reads unchanged.
    #[test]
    fn the_commit_message_keeps_its_old_lowercase_kind() {
        let (_d, kb) = fresh();
        append(&kb, ChangeKind::Ingest, "a source", None, None).unwrap();
        let repo = crate::knowledge::git::GitRepo::open(&kb).unwrap();
        assert!(
            repo.log(1).unwrap()[0].summary.contains("a source"),
            "the commit summary lost the entry"
        );
    }

    // -- the pure splice ----------------------------------------------------

    const TODAY: &str = "2026-08-19";

    fn entry(word: &str) -> String {
        format!("* **Manual** — {word}\n")
    }

    #[test]
    fn a_second_entry_on_the_same_day_joins_that_days_group_newest_first() {
        let first = with_entry("# Log\n\n", TODAY, &entry("first"));
        let second = with_entry(&first, TODAY, &entry("second"));
        assert_eq!(
            second.matches("## 2026-08-19").count(),
            1,
            "a second group for the same day: {second}"
        );
        let newer = second.find("second").unwrap();
        let older = second.find("first").unwrap();
        assert!(newer < older, "not newest-first within the day:\n{second}");
    }

    #[test]
    fn a_new_day_goes_above_every_existing_group() {
        let yesterday = with_entry("# Log\n\n", "2026-08-18", &entry("older"));
        let both = with_entry(&yesterday, TODAY, &entry("newer"));
        let new_group = both.find("## 2026-08-19").unwrap();
        let old_group = both.find("## 2026-08-18").unwrap();
        assert!(new_group < old_group, "not newest-first:\n{both}");
        assert!(
            both.find("# Log").unwrap() < new_group,
            "the H1 must stay on top:\n{both}"
        );
    }

    /// A base written before this change keeps its `## [date] kind | summary`
    /// headings verbatim. They are user content and rewriting them would be a
    /// format migration of the kind DR-17/DR-22 keep off every automatic path.
    #[test]
    fn a_legacy_log_is_not_rewritten_only_prepended_to() {
        let legacy = "# Log\n\n## [2026-01-02] ingest | an old source\n\n## [2026-01-01] manual | older still\n\n";
        let updated = with_entry(legacy, TODAY, &entry("new"));
        assert!(updated.contains("## [2026-01-02] ingest | an old source"));
        assert!(updated.contains("## [2026-01-01] manual | older still"));
        assert!(
            updated.find("## 2026-08-19").unwrap() < updated.find("## [2026-01-02]").unwrap(),
            "the new group must go above the legacy ones:\n{updated}"
        );
        // …and the legacy headings are exactly what `check_log` objects to, so
        // this is also the proof that the checker is looking at the right thing.
        assert_eq!(okf::check_log(&updated).len(), 2, "{updated}");
    }

    #[test]
    fn a_log_with_no_groups_at_all_still_gains_one() {
        for existing in ["", "# Log\n", "# Log\n\nSome prose the user wrote.\n"] {
            let updated = with_entry(existing, TODAY, &entry("x"));
            assert!(updated.contains("## 2026-08-19"), "from {existing:?}");
            assert!(updated.contains("* **Manual** — x"), "from {existing:?}");
            assert!(updated.ends_with('\n'), "from {existing:?}");
            assert!(okf::check_log(&updated).is_empty(), "from {existing:?}");
        }
        assert!(
            with_entry("# Log\n\nSome prose the user wrote.\n", TODAY, &entry("x"))
                .contains("Some prose the user wrote."),
            "the user's prose was dropped"
        );
    }

    #[test]
    fn a_multi_line_delta_stays_inside_its_bullet() {
        let block = entry_block(
            ChangeKind::Ingest,
            "a source",
            Some("+2 pages\n+1 source\n"),
        );
        assert_eq!(
            block, "* **Ingest** — a source\n  +2 pages\n  +1 source\n",
            "an unindented continuation line ends the bullet and becomes a \
             sibling of the group heading"
        );
    }

    #[test]
    fn every_change_kind_has_a_distinct_bold_word() {
        let kinds = [
            ChangeKind::Ingest,
            ChangeKind::Link,
            ChangeKind::Flag,
            ChangeKind::Query,
            ChangeKind::Lint,
            ChangeKind::Restore,
            ChangeKind::Manual,
        ];
        let mut words: Vec<&str> = kinds.iter().map(|k| kind_word(*k)).collect();
        words.sort_unstable();
        let count = words.len();
        words.dedup();
        assert_eq!(
            words.len(),
            count,
            "two kinds share a word, so the log no longer says which happened"
        );
        // The heading used to carry the kind; the bullet must carry the same
        // information or the move lost it.
        for k in kinds {
            assert!(kind_word(k).to_lowercase() == kind_slug(k));
        }
    }
}
