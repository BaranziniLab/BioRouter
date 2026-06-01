//! `lint` macro — deterministic KB hygiene scan + optional sub-agent autofix.
//!
//! Part A: `scan(kb_root)` — pure, synchronous, no LLM.
//! Part B: `lint(svc, args)` — calls `scan`, optionally runs a sub-agent to fix issues.

use crate::knowledge::{
    git::GitRepo,
    paths, raw,
    service::KnowledgeService,
    store::split_frontmatter,
    subagent::{
        events::{DoneReason, SubAgentEvent},
        kb_tools::{tool_specs, KbToolDispatch},
        loop_::{Completer, SubAgent, SubAgentBounds},
        procedures::LINT_PROCEDURE,
    },
    types::ChangeKind,
};
use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

// ---------------------------------------------------------------------------
// LintReport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LintReport {
    /// Pages with no inbound `[[wiki-link]]` references from any other page.
    pub orphans: Vec<String>,
    /// Pages with `contradiction: true` in frontmatter.
    pub contradictions: Vec<String>,
    /// Sources whose `ingested_at` is >90 days ago AND have no inbound links.
    pub stale_sources: Vec<String>,
    /// `[[Target]]` references in source pages that have no corresponding page
    /// under `knowledge/`.
    pub missing_concept_pages: Vec<String>,
}

// ---------------------------------------------------------------------------
// Part A: deterministic scan
// ---------------------------------------------------------------------------

pub fn scan(kb_root: &Path) -> Result<LintReport> {
    let knowledge_dir = kb_root.join("knowledge");
    if !knowledge_dir.exists() {
        return Ok(LintReport::default());
    }

    // Collect all pages and their bodies.
    let mut pages: HashMap<String, String> = HashMap::new(); // logical_path -> body
    collect_pages(&knowledge_dir, &knowledge_dir, &mut pages)?;

    // Build inbound-link map: target_path -> set of source paths that link to it.
    // We recognise `[[Name]]` and resolve to the first page whose filename stem
    // case-insensitively matches.
    let wiki_re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let mut inbound: HashMap<String, HashSet<String>> = HashMap::new();

    // Initialise all known pages with empty inbound sets.
    for p in pages.keys() {
        inbound.entry(p.clone()).or_default();
    }

    // Also track which wiki-link targets appear in source pages.
    let mut wiki_targets_in_sources: HashSet<String> = HashSet::new();

    for (src_path, body) in &pages {
        for cap in wiki_re.captures_iter(body) {
            let target_name = cap.get(1).unwrap().as_str();
            // Record as a target in source pages (used for missing-concept detection).
            if src_path.starts_with("knowledge/sources/") {
                wiki_targets_in_sources.insert(target_name.to_string());
            }
            // Try to resolve the wiki-link to a known page.
            if let Some(resolved) = resolve_wiki_link(&pages, target_name) {
                inbound
                    .entry(resolved)
                    .or_default()
                    .insert(src_path.clone());
            }
        }
    }

    // ---- Orphans: pages with no inbound links, excluding hub pages ----
    // Hub pages = any page directly under knowledge/ (not in a subdirectory).
    let orphans: Vec<String> = inbound
        .iter()
        .filter(|(path, sources)| sources.is_empty() && !is_hub_page(path))
        .map(|(path, _)| path.clone())
        .collect();

    // ---- Contradictions: pages with frontmatter contradiction: true ----
    let contradictions: Vec<String> = pages
        .iter()
        .filter(|(_, body)| {
            let (fm, _) = split_frontmatter(body);
            fm.get("contradiction")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .map(|(path, _)| path.clone())
        .collect();

    // ---- Stale sources: raw/ sources >90 days without inbound links ----
    let ninety_days = chrono::Duration::days(90);
    let now = Utc::now();
    let mut stale_sources: Vec<String> = Vec::new();
    if let Ok(metas) = raw::list_sources(kb_root) {
        for meta in metas {
            let age = now.signed_duration_since(meta.ingested_at);
            if age > ninety_days {
                // Check for any inbound reference by source-id or source page path.
                let source_page = format!("knowledge/sources/{}.md", meta.id);
                let has_inbound = inbound
                    .get(&source_page)
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);
                if !has_inbound {
                    stale_sources.push(meta.id.clone());
                }
            }
        }
    }

    // ---- Missing concept pages: wiki-link targets from source pages that
    //      don't have a page under knowledge/ ----
    let mut missing_concept_pages: Vec<String> = Vec::new();
    for target in &wiki_targets_in_sources {
        if resolve_wiki_link(&pages, target).is_none() && !missing_concept_pages.contains(target) {
            missing_concept_pages.push(target.clone());
        }
    }

    Ok(LintReport {
        orphans,
        contradictions,
        stale_sources,
        missing_concept_pages,
    })
}

// ---- helpers ----------------------------------------------------------------

fn collect_pages(base: &Path, dir: &Path, out: &mut HashMap<String, String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_pages(base, &p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            let logical = format!("knowledge/{rel}");
            let body = std::fs::read_to_string(&p)?;
            out.insert(logical, body);
        }
    }
    Ok(())
}

/// Returns true for pages directly under `knowledge/` (no subdirectory).
fn is_hub_page(path: &str) -> bool {
    // "knowledge/index.md" has exactly one '/' after "knowledge/".
    let rest = path.strip_prefix("knowledge/").unwrap_or(path);
    !rest.contains('/')
}

/// Try to resolve a wiki-link target name to a logical page path.
/// Matching strategy: case-insensitive stem comparison.
fn resolve_wiki_link(pages: &HashMap<String, String>, target: &str) -> Option<String> {
    let target_lower = target.to_ascii_lowercase();
    // Replace spaces with hyphens as a normalisation step.
    let target_slug = target_lower.replace(' ', "-");
    pages
        .keys()
        .find(|path| {
            // Extract the file stem from the logical path.
            let stem = Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            stem == target_lower || stem == target_slug
        })
        .cloned()
}

// ---------------------------------------------------------------------------
// Part B: async lint (with optional sub-agent autofix)
// ---------------------------------------------------------------------------

pub struct LintArgs {
    pub kb_id: String,
    pub completer: Option<Box<dyn Completer>>,
    pub autofix: bool,
    pub bounds: SubAgentBounds,
    /// Optional channel for live event streaming. When provided, every
    /// `SubAgentEvent` is sent here as soon as it is produced (not just at
    /// the end of the run). Set to `None` if streaming is not needed.
    pub event_sink: Option<tokio::sync::mpsc::UnboundedSender<SubAgentEvent>>,
    /// Optional cancellation signal. When `notify_one()` is called on the
    /// shared `Notify`, the sub-agent loop returns `DoneReason::Cancelled`
    /// at the start of its next iteration.
    pub cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LintResult {
    pub report: LintReport,
    pub commit_sha: Option<String>,
    pub fixes_applied: usize,
}

pub async fn lint(svc: &KnowledgeService, args: LintArgs) -> Result<LintResult> {
    let _lock = svc.lock_kb(&args.kb_id).await;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    let report = scan(&kb_root)?;

    if !args.autofix {
        return Ok(LintResult {
            report,
            commit_sha: None,
            fixes_applied: 0,
        });
    }

    let completer = args
        .completer
        .ok_or_else(|| anyhow::anyhow!("completer required for autofix"))?;

    let repo = GitRepo::open(&kb_root)?;
    let txn = repo.begin_txn("lint")?;

    let schema = std::fs::read_to_string(kb_root.join("schema.md")).context("read schema.md")?;
    let report_json = serde_json::to_string_pretty(&serde_json::json!({
        "orphans": report.orphans,
        "contradictions": report.contradictions,
        "stale_sources": report.stale_sources,
        "missing_concept_pages": report.missing_concept_pages,
    }))?;
    let system = format!("{schema}\n\n---\n{LINT_PROCEDURE}");
    let user = format!(
        "autofix=true. Here is the current lint report:\n```json\n{report_json}\n```\nPlease fix the issues."
    );

    let dispatch = KbToolDispatch {
        svc: svc.clone(),
        kb_id: args.kb_id.clone(),
        txn_branch: txn.branch.clone(),
    };
    let agent = SubAgent {
        completer,
        tools: tool_specs(),
        system_prompt: system,
        bounds: args.bounds,
    };

    let cancel_ref = args.cancel.as_deref();
    let agent_result = agent
        .run(&user, &dispatch, cancel_ref, args.event_sink.as_ref())
        .await;

    match agent_result {
        Ok(r)
            if matches!(
                r.reason,
                DoneReason::CompleteSentinel | DoneReason::NoMoreToolCalls
            ) =>
        {
            let fixes_applied = r
                .events
                .iter()
                .filter(|e| {
                    matches!(e, SubAgentEvent::ToolResult { name, ok: true, .. }
                        if name == "kb_write_page")
                })
                .count();
            let sha = repo.commit_txn(&txn, ChangeKind::Lint, "lint autofix", None)?;
            Ok(LintResult {
                report,
                commit_sha: Some(sha),
                fixes_applied,
            })
        }
        Ok(r) => {
            let _ = repo.abort_txn(&txn);
            anyhow::bail!(
                "lint sub-agent aborted: reason={:?}, final={}",
                r.reason,
                r.final_text
            )
        }
        Err(e) => {
            let _ = repo.abort_txn(&txn);
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{service::KnowledgeService, store::write_page};

    fn fresh_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir, svc)
    }

    // ---- Part A: scan tests -------------------------------------------------

    /// A page with no inbound links from other pages should be detected as an orphan.
    #[test]
    fn scan_finds_orphan_page() {
        let (_dir, svc) = fresh_svc();
        let kb = svc.root().join("k");

        // Page A links to page B. Page C has no inbound links → orphan.
        write_page(
            &kb,
            "knowledge/entities/a.md",
            "---\ntitle: A\nkind: entity\n---\n\nSee also [[B]].",
            "add a",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/entities/b.md",
            "---\ntitle: B\nkind: entity\n---\n\nB body.",
            "add b",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/entities/c.md",
            "---\ntitle: C\nkind: entity\n---\n\nC body with no inbound links.",
            "add c",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        let orphan_paths: Vec<_> = report.orphans.iter().map(|s| s.as_str()).collect();
        assert!(
            orphan_paths.contains(&"knowledge/entities/c.md"),
            "page c should be orphaned; orphans={orphan_paths:?}"
        );
        // Page B is referenced by A so it must NOT be an orphan.
        assert!(
            !orphan_paths.contains(&"knowledge/entities/b.md"),
            "page b is referenced by a — not an orphan"
        );
    }

    /// A page with `contradiction: true` in frontmatter should appear in `contradictions`.
    #[test]
    fn scan_finds_contradiction_flag() {
        let (_dir, svc) = fresh_svc();
        let kb = svc.root().join("k");

        write_page(
            &kb,
            "knowledge/concepts/disputed.md",
            "---\ntitle: Disputed\nkind: concept\ncontradiction: true\n---\n\nConflicting claims.",
            "add disputed",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/concepts/normal.md",
            "---\ntitle: Normal\nkind: concept\n---\n\nNormal body.",
            "add normal",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        let contra_paths: Vec<_> = report.contradictions.iter().map(|s| s.as_str()).collect();
        assert!(
            contra_paths.contains(&"knowledge/concepts/disputed.md"),
            "disputed.md should appear in contradictions; got {contra_paths:?}"
        );
        assert!(
            !contra_paths.contains(&"knowledge/concepts/normal.md"),
            "normal.md must not appear in contradictions"
        );
    }

    /// A source page that references `[[Z]]` but no `knowledge/concepts/z.md` exists
    /// should produce "Z" in `missing_concept_pages`.
    #[test]
    fn scan_finds_missing_concept() {
        let (_dir, svc) = fresh_svc();
        let kb = svc.root().join("k");

        // A source page that references [[Z]] (no knowledge/concepts/z.md exists).
        write_page(
            &kb,
            "knowledge/sources/paper-x.md",
            "---\ntitle: Paper X\nkind: source\n---\n\nThis paper mentions [[Z]] frequently.",
            "add paper",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        assert!(
            report
                .missing_concept_pages
                .iter()
                .any(|m| m.eq_ignore_ascii_case("Z")),
            "missing_concept_pages should contain 'Z'; got {:?}",
            report.missing_concept_pages
        );
    }

    // ---- Part B: async lint tests ------------------------------------------

    /// scan-only (autofix=false) should return the report with no commit.
    #[tokio::test]
    async fn lint_scan_only_returns_report_no_commit() {
        let (_dir, svc) = fresh_svc();
        let kb = svc.root().join("k");

        // Create one orphan page.
        write_page(
            &kb,
            "knowledge/entities/lonely.md",
            "---\ntitle: Lonely\nkind: entity\n---\n\nNo one links to me.",
            "add lonely",
            None,
        )
        .unwrap();

        let result = lint(
            &svc,
            LintArgs {
                kb_id: "k".into(),
                completer: None,
                autofix: false,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .unwrap();

        assert!(
            !result.report.orphans.is_empty(),
            "report should contain orphans"
        );
        assert!(
            result.commit_sha.is_none(),
            "scan-only must not produce a commit"
        );
        assert_eq!(result.fixes_applied, 0);
    }
}
