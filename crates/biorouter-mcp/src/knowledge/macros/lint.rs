//! `lint` macro — deterministic KB hygiene scan + optional sub-agent autofix.
//!
//! Part A: `scan(kb_root)` — pure, synchronous, no LLM.
//! Part B: `lint(svc, args)` — calls `scan`, optionally runs a sub-agent to fix issues.

use crate::knowledge::{
    biookf,
    git::{GitRepo, Txn},
    graph, manifest, okf, paths, raw,
    service::KnowledgeService,
    store::{logical_path, split_frontmatter},
    subagent::{
        events::{DoneReason, SubAgentEvent},
        kb_tools::{tool_specs, KbToolDispatch},
        loop_::{Completer, SubAgent, SubAgentBounds, SubAgentResult},
        procedures::{lint_procedure, system_prompt},
    },
    types::ChangeKind,
    types::KbFormat,
    validate::{BundlePage, Diagnostic, Diagnostics, Severity},
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::{collections::HashMap, path::Path};

// ---------------------------------------------------------------------------
// LintReport
// ---------------------------------------------------------------------------

/// The four hygiene rules that predate the format layers, as stable ids.
///
/// Prefixed `kb.` rather than `okf.` / `biookf.` because they are BioRouter's
/// own housekeeping and not a clause of either spec: no OKF rule says a page
/// must be linked to, and §11 rule 4 explicitly forbids rejecting a bundle for
/// a broken cross-link. Keeping the prefixes apart is what lets a reader — and a
/// Stage 6 UI — tell "your base is untidy" from "this file is not conformant".
pub const RULE_ORPHAN: &str = "kb.orphan";
pub const RULE_CONTRADICTION: &str = "kb.contradiction";
pub const RULE_STALE_SOURCE: &str = "kb.stale_source";
pub const RULE_MISSING_CONCEPT_PAGE: &str = "kb.missing_concept_page";

/// The payload of the lint stream's terminal `event: done` frame, and — since
/// Stage 6 — a published schema.
///
/// It is `ToSchema` rather than merely `Serialize` because
/// `POST /knowledge/bases/{id}/lint` answers over SSE, so the shape a client
/// has to parse cannot be inferred from the response body's own type. Declaring
/// it puts the four lists **and** the typed [`Diagnostics`] into
/// `components.schemas`, and therefore into the generated TypeScript, where the
/// renderer reads the frame. A hand-written interface in `ui/` would be the
/// same contract with nothing keeping it in step.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LintReport {
    /// Pages nothing else in the bundle links to, in any of the four grammars
    /// (`graph::bundle_links`) — not the legacy bracket form alone, which is
    /// what made this list every page of a typed base.
    pub orphans: Vec<String>,
    /// Pages with `contradiction: true` in frontmatter.
    pub contradictions: Vec<String>,
    /// Sources whose `ingested_at` is >90 days ago AND have no inbound links.
    pub stale_sources: Vec<String>,
    /// Link targets written in source pages that name no page under
    /// `knowledge/` — again over all four grammars, so a typed base's `edges:`
    /// citations are read.
    pub missing_concept_pages: Vec<String>,
    /// Everything above, plus the format layers, as one typed list: a stable
    /// rule id, a severity, the page or edge it is about, and a message.
    ///
    /// ⚠ **It does not replace the four lists, and that is deliberate.** They
    /// are the shape `LINT_PROCEDURE` teaches the autofix sub-agent and the
    /// shape a stored report deserializes into; dropping them would rewrite a
    /// working prompt under a change whose subject is structure. The four
    /// re-appear here as `kb.*` diagnostics, so a consumer reads either and a
    /// new consumer reads only this.
    ///
    /// `serde(default)` so a report written before Stage 4 still loads.
    #[serde(default)]
    pub diagnostics: Diagnostics,
}

// ---------------------------------------------------------------------------
// Part A: deterministic scan
// ---------------------------------------------------------------------------

pub fn scan(kb_root: &Path) -> Result<LintReport> {
    let knowledge_dir = kb_root.join("knowledge");
    if !knowledge_dir.exists() {
        return Ok(LintReport::default());
    }

    // Collect all pages and their bodies — for the checks that read a page's own
    // frontmatter and text. The *links* come from the deriver; see below.
    let mut pages: HashMap<String, String> = HashMap::new(); // logical_path -> body
    collect_pages(&knowledge_dir, &knowledge_dir, &mut pages)?;

    // Both link-shaped rules — "does anything link to this page?" and "does this
    // target name a page?" — are answered off the graph deriver's own edge set
    // (`graph::bundle_links`), which reads all four grammars and resolves through
    // DR-3's identity ladder.
    //
    // Lint used to walk the tree itself for `okf::links`' legacy bracket form
    // alone. That is exactly right for a legacy base and blind on a typed one,
    // where relationships live in BioOKF §6's `edges:` frontmatter array: a
    // BioOKF base of 11 pages and 17 typed edges reported **all 11 as orphans**
    // while the subject band beside it said "17 links" — lint sending the user to
    // fix every page of a base that was fine, and contradicting the graph drawn
    // next to it. Reading the deriver's answer rather than re-deriving one is
    // what makes a fifth grammar impossible to add to one and not the other.
    let links = graph::bundle_links(kb_root)?;
    let inbound = &links.inbound;

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

    // ---- Missing concept pages: link targets written in source pages that name
    //      no page under knowledge/ ----
    //
    // The same widening, for the same reason: on a typed base the citations a
    // source page makes are `edges:` entries, so a bracket-only reader found
    // none of them and this list was empty on exactly the bases the migration
    // produces. `unresolved` is the deriver's own set of non-resolutions, which
    // is why a target the graph draws an edge to can no longer appear here.
    let mut missing_concept_pages: Vec<String> = Vec::new();
    for (src_path, target) in &links.unresolved {
        if src_path.starts_with("knowledge/sources/") && !missing_concept_pages.contains(target) {
            missing_concept_pages.push(target.clone());
        }
    }

    let mut report = LintReport {
        orphans,
        contradictions,
        stale_sources,
        missing_concept_pages,
        diagnostics: Diagnostics::default(),
    };
    // Three of the four are collected out of a `HashMap`/`HashSet`, so before
    // this line the same base lint into a different order on every run. It was
    // invisible while the report was four bags of strings a human skimmed; it
    // stops being invisible the moment the entries are diagnostics, because a
    // diff of two reports becomes noise and the `MAX_DIAGNOSTICS` cut picks a
    // different subset each time. Sorting here rather than at each collection
    // site keeps it one statement about the whole report.
    report.orphans.sort();
    report.contradictions.sort();
    report.stale_sources.sort();
    report.missing_concept_pages.sort();
    report.diagnostics = Diagnostics::new(
        scan_diagnostics(&report)
            .chain(format_diagnostics(kb_root, &pages))
            .collect(),
    );
    Ok(report)
}

/// The four deterministic lists, restated as typed diagnostics.
///
/// Built from the report rather than from the loops that produced it, so the two
/// halves cannot disagree about what was found: there is one place a page
/// becomes an orphan, and this reads its answer.
fn scan_diagnostics(report: &LintReport) -> impl Iterator<Item = Diagnostic> + '_ {
    let rule = |rule: &'static str, severity, message: &'static str| {
        move |subject: &String| Diagnostic::scan(rule, severity, subject.clone(), message)
    };
    report
        .orphans
        .iter()
        .map(rule(
            RULE_ORPHAN,
            Severity::Warning,
            "no other page links to this one; link it from a hub page or remove it",
        ))
        .chain(report.contradictions.iter().map(rule(
            RULE_CONTRADICTION,
            Severity::Warning,
            "the page declares `contradiction: true`; resolve it or record which source wins",
        )))
        .chain(report.stale_sources.iter().map(rule(
            RULE_STALE_SOURCE,
            Severity::Info,
            "ingested over 90 days ago and still unreferenced by any page",
        )))
        .chain(report.missing_concept_pages.iter().map(rule(
            RULE_MISSING_CONCEPT_PAGE,
            Severity::Warning,
            "a source page links to this target, but no page under knowledge/ carries it",
        )))
}

/// The OKF layer over every page, plus the BioOKF profile when the base is in
/// it.
///
/// **A legacy base gets nothing** (DR-26). Its pages are `title`/`kind`
/// frontmatter and `[[wiki]]` links, which this build has promised never to
/// rewrite — so `okf.type.missing` on every one of them would report a decision
/// as several hundred defects and bury the four hygiene findings that are real.
/// `Manifest::profile` is the accessor that answers this and `Manifest::format`
/// is the trap: it reads `Okf` on every base written before Stage 3.
///
/// An unreadable manifest is treated as legacy, for the same reason: guessing
/// `Okf` for a base whose generation we could not establish produces exactly the
/// flood the paragraph above describes.
fn format_diagnostics(kb_root: &Path, pages: &HashMap<String, String>) -> Vec<Diagnostic> {
    let Some(format) = manifest::load(kb_root).ok().and_then(|m| m.profile()) else {
        return Vec::new();
    };
    let mut checked: Vec<(&String, &String, Option<okf::Page>)> = pages
        .iter()
        .filter(|(path, _)| !graph::is_scaffold_page(path))
        .map(|(path, text)| (path, text, okf::Page::parse(text).ok()))
        .collect();
    // `HashMap` iteration order is random, so without this the same base lints
    // into a different order on every run — which turns a diff of two reports
    // into noise and makes the `MAX_DIAGNOSTICS` cut pick a different subset
    // each time.
    checked.sort_by(|a, b| a.0.cmp(b.0));

    let subject_of = |path: &str, page: &Option<okf::Page>| {
        page.as_ref()
            .and_then(|p| p.doc.primary_key())
            .unwrap_or(path)
            .to_string()
    };
    let mut out: Vec<Diagnostic> = Vec::new();
    for (path, text, page) in &checked {
        let subject = subject_of(path, page);
        out.extend(
            okf::check_source(text)
                .into_iter()
                .map(|d| Diagnostic::from_okf_at(d, &subject, Some(path))),
        );
    }
    if format == KbFormat::Biookf {
        // One index for the whole run, which is what makes the cross-document
        // rules (duplicate `identifier`, unresolved `object` / `primary_source`)
        // answerable at all — and O(bundle) once rather than per page.
        let bundle: Vec<BundlePage> = checked
            .iter()
            .filter_map(|(path, _, page)| {
                page.as_ref().map(|p| BundlePage {
                    path: (*path).clone(),
                    doc: p.doc.clone(),
                })
            })
            .collect();
        let index = biookf::BundleIndex::build(bundle.iter().map(|p| (p.path.as_str(), &p.doc)));
        for p in &bundle {
            out.extend(
                biookf::check_doc(Some(&p.path), &p.doc, &index)
                    .findings
                    .into_iter()
                    .map(Diagnostic::from),
            );
        }
    }
    out
}

// ---- helpers ----------------------------------------------------------------

fn collect_pages(base: &Path, dir: &Path, out: &mut HashMap<String, String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_pages(base, &p, out)?;
        } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
            let logical = logical_path("knowledge", p.strip_prefix(base).unwrap());
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

// `resolve_wiki_link` lived here: a case-insensitive stem comparison with
// spaces replaced by hyphens, which could not read a path-style target at all.
// It became `knowledge::links::LinkIndex::resolve` — the graph's resolver, the
// most complete of the three and the only one that had tests — and is now
// `graph::bundle_links`, the deriver's whole answer rather than one rung of it.
// The intermediate step fixed *how* a target resolved and left *which* links
// were read alone, which is why a typed base still linted as all orphans.

// ---------------------------------------------------------------------------
// Part B: async lint (with optional sub-agent autofix)
// ---------------------------------------------------------------------------

pub struct LintArgs {
    pub kb_id: String,
    /// The capability of the model this macro will run (issue #56). Required,
    /// so every production caller is a compile error rather than an omission.
    pub caller_is_private: bool,
    /// Whose agreements cover that model — DR-26's third axis (issue #56, Task
    /// 50). Required for the same reason `caller_is_private` is: an omission
    /// must be a compile error. `Unstated` is its `Default`, so a caller that
    /// cannot determine one fails closed.
    pub caller_affiliation: crate::knowledge::affiliation::CallerAffiliation,
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

/// What the lint stream's terminal `event: done` frame actually carries.
///
/// The [`LintReport`] is nested rather than flattened because a lint answers two
/// questions — what is wrong, and what was changed about it — and an autofix
/// that rewrote pages has a `commit_sha` the report itself cannot express.
/// Published as a schema for the reason its `report` field is; see
/// [`LintReport`].
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LintResult {
    pub report: LintReport,
    /// The autofix commit, or `None` for a read-only lint — which is every lint
    /// that ran without `autofix`, and therefore without a provider at all.
    pub commit_sha: Option<String>,
    pub fixes_applied: usize,
}

pub async fn lint(svc: &KnowledgeService, args: LintArgs) -> Result<LintResult> {
    let _lock = svc.lock_kb(&args.kb_id).await?;
    // Issue #56. Before the sub-agent, not after: an autofix that fails halfway
    // has already written pages. Task 10C (CP2) puts the barrier on the line
    // above — a lint's `scan` reads every page, and an autofix rewrites them.
    crate::knowledge::tier::assert_reachable(
        svc.root(),
        &args.kb_id,
        args.caller_is_private,
        &args.caller_affiliation,
    )?;
    // Issue #56, both axes in one call under one lock — see
    // `KnowledgeService::raise_tier_and_affiliation` for why they cannot be
    // two.
    svc.raise_tier_and_affiliation(
        &args.kb_id,
        args.caller_is_private,
        &args.caller_affiliation,
    )?;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    // Migrate a stale `schema.md` (the sub-agent's system prompt) and refresh a
    // stale graph cache, neither fatally. See `macros::refresh_base`.
    super::refresh_base(svc, &args.kb_id);

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
    // The four lists are what `LINT_PROCEDURE` teaches, so they stay exactly as
    // they were. `diagnostics` is added beside them rather than instead of them,
    // and is already capped at `MAX_DIAGNOSTICS` — an uncapped list over a large
    // base is a context-window failure that presents as the model losing track.
    let report_json = serde_json::to_string_pretty(&serde_json::json!({
        "orphans": report.orphans,
        "contradictions": report.contradictions,
        "stale_sources": report.stale_sources,
        "missing_concept_pages": report.missing_concept_pages,
        "diagnostics": report.diagnostics,
    }))?;
    // See `query`'s note on `profile()` vs `format`: an autofix run on a legacy
    // base must not be handed BioOKF's typed writer.
    let format = manifest::load(&kb_root).ok().and_then(|m| m.profile());
    let system = system_prompt(&schema, lint_procedure(format));
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
        tools: tool_specs(format),
        system_prompt: system,
        bounds: args.bounds,
    };

    let cancel_ref = args.cancel.as_deref();
    let agent_result = agent
        .run(&user, &dispatch, cancel_ref, args.event_sink.as_ref())
        .await;

    settle_autofix(svc, &args.kb_id, &repo, &txn, report, agent_result)
}

/// The autofix transaction's three endings — commit, abort-and-report,
/// abort-and-fail — split out of [`lint`] so that function stays under
/// `clippy::too_many_lines` (issue #56, review round 5). No behaviour change.
fn settle_autofix(
    svc: &KnowledgeService,
    kb_id: &str,
    repo: &GitRepo,
    txn: &Txn,
    report: LintReport,
    agent_result: Result<SubAgentResult>,
) -> Result<LintResult> {
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
            // An autofix run that fixed nothing must not commit. `commit_txn`
            // squash-commits the transaction *tree* and git records a commit
            // whose tree equals its parent's without complaint, so every clean
            // KB used to gain an empty `[lint] lint autofix` entry — visible in
            // the Knowledge change-log drawer as work that never happened, and
            // a `commit_sha` callers read as proof of it. Same false success as
            // issue #71, one macro over.
            let fixed = match repo.txn_wrote_knowledge_pages(txn) {
                Ok(fixed) => fixed,
                Err(e) => {
                    let _ = repo.abort_txn(txn);
                    return Err(e.context("checking whether the lint fixed anything"));
                }
            };
            if !fixed {
                let _ = repo.abort_txn(txn);
                return Ok(LintResult {
                    report,
                    commit_sha: None,
                    fixes_applied,
                });
            }
            let sha = repo.commit_txn(txn, ChangeKind::Lint, "lint autofix", None)?;
            svc.rebuild_graph_cache(kb_id)?;
            Ok(LintResult {
                report,
                commit_sha: Some(sha),
                fixes_applied,
            })
        }
        Ok(r) => {
            let _ = repo.abort_txn(txn);
            anyhow::bail!(
                "lint sub-agent aborted: reason={:?}, final={}",
                r.reason,
                r.final_text
            )
        }
        Err(e) => {
            let _ = repo.abort_txn(txn);
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
    use crate::knowledge::{
        affiliation::CallerAffiliation, service::KnowledgeService, store::write_page,
    };

    fn fresh_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir, svc)
    }

    /// A base in a named profile, created through the production path so its
    /// `schema.md`, its scaffold directories and its `schema_version` are the
    /// real ones and not a hand-built approximation.
    fn svc_in(format: KbFormat) -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_as("k", "K", None, format, false, &CallerAffiliation::Unstated)
            .unwrap();
        (dir, svc)
    }

    /// A base written before the OKF generation: the shape every base on disk
    /// has today. `schema_version` is what separates it, not `format` — see
    /// `Manifest::profile`.
    fn make_legacy(svc: &KnowledgeService) {
        let kb = svc.root().join("k");
        let mut m = manifest::load(&kb).unwrap();
        m.schema_version = 1;
        manifest::save(&kb, &m).unwrap();
    }

    fn rules(report: &LintReport) -> Vec<&str> {
        report
            .diagnostics
            .items
            .iter()
            .map(|d| d.rule.as_str())
            .collect()
    }

    // ---- Stage 4: typed diagnostics ----------------------------------------

    /// The structure is additive. The four lists keep their contents and their
    /// meaning, and each entry re-appears as a `kb.*` diagnostic carrying the
    /// same subject — so a consumer reads either and neither can drift.
    #[test]
    fn the_four_lists_survive_and_re_appear_as_typed_diagnostics() {
        let (_dir, svc) = fresh_svc();
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/entities/c.md",
            "---\ntitle: C\nkind: entity\ncontradiction: true\n---\n\nNothing links here.",
            "add c",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        assert_eq!(report.orphans, vec!["knowledge/entities/c.md"]);
        assert_eq!(report.contradictions, vec!["knowledge/entities/c.md"]);

        for rule in [RULE_ORPHAN, RULE_CONTRADICTION] {
            let found = report
                .diagnostics
                .items
                .iter()
                .find(|d| d.rule == rule)
                .unwrap_or_else(|| panic!("{rule} missing from {:?}", rules(&report)));
            assert_eq!(found.subject, "knowledge/entities/c.md");
            assert_eq!(found.path.as_deref(), Some("knowledge/entities/c.md"));
            assert!(!found.message.is_empty());
        }
    }

    /// DR-26. A legacy base is read through its own generation's path and never
    /// rewritten, so it gets the four hygiene rules and **no format layer** —
    /// otherwise every one of its `title`/`kind` pages reports `okf.type.missing`
    /// and the findings that are real are buried under a decision.
    #[test]
    fn a_legacy_base_is_linted_for_hygiene_and_not_for_conformance() {
        let (_dir, svc) = fresh_svc();
        make_legacy(&svc);
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/entities/c.md",
            "---\ntitle: C\nkind: entity\n---\n\nNothing links here.",
            "add c",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        assert_eq!(rules(&report), vec![RULE_ORPHAN]);
        assert!(
            !rules(&report).iter().any(|r| r.starts_with("okf.")),
            "a legacy base was checked against OKF: {:?}",
            rules(&report)
        );
    }

    /// The same page in an OKF base **is** checked, which is what makes the
    /// silence above the profile answering rather than the checker doing
    /// nothing.
    #[test]
    fn an_okf_base_reports_the_format_layer_too() {
        let (_dir, svc) = svc_in(KbFormat::Okf);
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/concept/c.md",
            "---\ntitle: C\nkind: entity\n---\n\nNothing links here.",
            "add c",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        assert!(
            report
                .diagnostics
                .has(crate::knowledge::okf::conformance::RULE_TYPE_MISSING),
            "{:?}",
            rules(&report)
        );
    }

    /// DR-24, end to end and at the bundle level: a base an ingest produced —
    /// materialised source node, concept pages citing it, `reported_in` edges
    /// back to it — lints with **no** unresolved provenance, and the source
    /// node's self-citing `reported_in` is not reported as a dangling source.
    ///
    /// The self-reference is the part that needs an assertion. It is the
    /// intended terminating base case (SPEC §8.1: a source attests its own
    /// contents), and a lint that treated it as an ordinary citation would flag
    /// **every source in every base** — a finding per source, on the one page
    /// that can never be fixed, drowning the findings that are real.
    #[test]
    fn a_bundle_with_materialized_sources_has_no_unresolved_provenance() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/publication/chen-2020.md",
            "---\ntype: Publication\nidentifier: Chen 2020\nxref: [PMID:32504360]\n\
             raw_source: [raw/chen-2020/source.md]\nedges:\n  - predicate: reported_in\n    \
             object: Chen 2020\n    knowledge_level: knowledge_assertion\n    \
             agent_type: automated_agent\n    primary_source: Chen 2020\n---\n\n# Chen 2020\n",
            "add source node",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/disease/covid-19.md",
            "---\ntype: Disease\nidentifier: COVID-19\nedges:\n  - predicate: reported_in\n    \
             object: Chen 2020\n    knowledge_level: knowledge_assertion\n    \
             agent_type: text_mining_agent\n    primary_source: Chen 2020\n---\n\n# COVID-19\n",
            "add disease",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/molecule/interleukin-6.md",
            "---\ntype: Molecule\nidentifier: Interleukin-6\nedges:\n  \
             - predicate: associated_with\n    object: COVID-19\n    \
             knowledge_level: statistical_association\n    agent_type: text_mining_agent\n    \
             primary_source: Chen 2020\n    p_value: 3.0e-6\n  - predicate: reported_in\n    \
             object: Chen 2020\n    knowledge_level: knowledge_assertion\n    \
             agent_type: text_mining_agent\n    primary_source: Chen 2020\n---\n\n# Interleukin-6\n",
            "add molecule",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        for rule in [
            crate::knowledge::biookf::lint::RULE_EDGE_PRIMARY_SOURCE_UNRESOLVED,
            crate::knowledge::biookf::lint::RULE_EDGE_PRIMARY_SOURCE_NOT_SOURCE,
            crate::knowledge::biookf::lint::RULE_EDGE_OBJECT_UNRESOLVED,
            crate::knowledge::biookf::lint::RULE_SOURCE_UNANCHORED,
        ] {
            assert!(
                !report.diagnostics.has(rule),
                "{rule} fired on a bundle whose sources are all materialised: {:#?}",
                report.diagnostics.items
            );
        }
        assert_eq!(
            report
                .diagnostics
                .count(crate::knowledge::validate::Severity::Error),
            0,
            "{:#?}",
            report.diagnostics.items
        );
    }

    /// The counterexample that makes the test above mean something: drop the
    /// source node and every edge citing it is reported. This is precisely what
    /// a BioOKF ingest looked like before DR-24 was implemented — one omission,
    /// one finding per edge, and nothing in the report naming the omission.
    #[test]
    fn without_the_source_node_every_citing_edge_is_reported() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/disease/covid-19.md",
            "---\ntype: Disease\nidentifier: COVID-19\nedges:\n  - predicate: reported_in\n    \
             object: Chen 2020\n    knowledge_level: knowledge_assertion\n    \
             agent_type: text_mining_agent\n    primary_source: Chen 2020\n---\n\n# COVID-19\n",
            "add disease",
            None,
        )
        .unwrap();
        let report = scan(&kb).unwrap();
        assert!(report
            .diagnostics
            .has(crate::knowledge::biookf::lint::RULE_EDGE_PRIMARY_SOURCE_UNRESOLVED));
    }

    /// The vocabulary layer only runs in BioOKF mode, and it runs against a
    /// bundle index — so an edge into a page that exists resolves, and only the
    /// invented predicate is reported.
    #[test]
    fn a_biookf_base_reports_the_vocabulary_and_resolves_real_edges() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/dataset/drugbank.md",
            "---\ntype: Dataset\nidentifier: DrugBank\nxref: [infores:drugbank]\n---\n\n# DrugBank\n",
            "add source node",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/disease/headache.md",
            "---\ntype: Disease\nidentifier: Headache\n---\n\nSee [[Aspirin]].\n",
            "add disease",
            None,
        )
        .unwrap();
        write_page(
            &kb,
            "knowledge/molecule/aspirin.md",
            "---\ntype: Molecule\nidentifier: Aspirin\nedges:\n  - predicate: treats\n    \
             object: Headache\n    knowledge_level: knowledge_assertion\n    \
             agent_type: manual_agent\n    primary_source: DrugBank\n  - predicate: heals\n    \
             object: Headache\n    knowledge_level: knowledge_assertion\n    \
             agent_type: manual_agent\n    primary_source: DrugBank\n---\n\nSee [[Headache]].\n",
            "add molecule",
            None,
        )
        .unwrap();

        let report = scan(&kb).unwrap();
        assert!(
            report
                .diagnostics
                .has(crate::knowledge::biookf::lint::RULE_PREDICATE_INVALID),
            "{:?}",
            rules(&report)
        );
        assert!(
            !report
                .diagnostics
                .has(crate::knowledge::biookf::lint::RULE_EDGE_OBJECT_UNRESOLVED),
            "`treats -> Headache` should resolve against the bundle: {:?}",
            report.diagnostics.items
        );
        // The invalid predicate names a page, so the finding is actionable.
        let finding = report
            .diagnostics
            .items
            .iter()
            .find(|d| d.rule == crate::knowledge::biookf::lint::RULE_PREDICATE_INVALID)
            .unwrap();
        assert_eq!(finding.subject, "Aspirin");
        assert_eq!(
            finding.path.as_deref(),
            Some("knowledge/molecule/aspirin.md")
        );
    }

    /// `HashMap` iteration order is random. Without the sort in
    /// `format_diagnostics` the same base lints into a different order every
    /// run, which turns a diff of two reports into noise and makes the
    /// `MAX_DIAGNOSTICS` cut pick a different subset each time.
    #[test]
    fn two_scans_of_one_base_produce_the_same_diagnostics_in_the_same_order() {
        let (_dir, svc) = svc_in(KbFormat::Okf);
        let kb = svc.root().join("k");
        for i in 0..12 {
            write_page(
                &kb,
                &format!("knowledge/concept/p{i}.md"),
                "---\ntitle: P\n---\n\nbody\n",
                "add",
                None,
            )
            .unwrap();
        }
        let first = scan(&kb).unwrap().diagnostics;
        let second = scan(&kb).unwrap().diagnostics;
        assert_eq!(first, second);
        assert!(first.total >= 12);
    }

    /// A report written before Stage 4 has no `diagnostics` key at all.
    #[test]
    fn a_report_without_the_new_field_still_deserializes() {
        let legacy = r#"{"orphans":["knowledge/a.md"],"contradictions":[],
                         "stale_sources":[],"missing_concept_pages":[]}"#;
        let report: LintReport = serde_json::from_str(legacy).unwrap();
        assert_eq!(report.orphans, vec!["knowledge/a.md"]);
        assert!(report.diagnostics.is_empty());
    }

    // ---- The link-grammar widening ------------------------------------------

    /// One typed page: `type` + `identifier`, and an `edges:` array with the
    /// three required attributes. Deliberately carries **no bracket link at
    /// all**, because that is the shape of a base this migration produces and
    /// the shape the bracket-only reader was blind to.
    fn typed_page(kind: &str, id: &str, edges: &[(&str, &str)]) -> String {
        let mut fm = format!("---\ntype: {kind}\nidentifier: {id}\n");
        if !edges.is_empty() {
            fm.push_str("edges:\n");
            for (predicate, object) in edges {
                fm.push_str(&format!(
                    "  - predicate: {predicate}\n    object: {object}\n    \
                     knowledge_level: knowledge_assertion\n    \
                     agent_type: manual_agent\n    primary_source: DrugBank\n"
                ));
            }
        }
        fm.push_str(&format!("---\n\n# {id}\n\nProse with no links in it.\n"));
        fm
    }

    fn write(kb: &Path, path: &str, body: &str) {
        write_page(kb, path, body, "add", None).unwrap();
    }

    /// The defect, measured in the running app: a BioOKF base whose pages link
    /// to each other **only** through `edges:` frontmatter had every page
    /// reported as an orphan, while the subject band beside it counted the typed
    /// edges. Lint was reading one grammar and the graph four.
    ///
    /// The cycle is what makes this fail loudly against the old reader: every
    /// page has an inbound typed edge, so a correct lint reports **no** orphan
    /// and a bracket-only one reports all three. `lonely` is the control in the
    /// next test — without it "no orphans" is also what a check that stopped
    /// firing would print.
    #[test]
    fn a_base_linked_only_by_typed_edges_has_no_orphans() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write(
            &kb,
            "knowledge/molecule/aspirin.md",
            &typed_page("Molecule", "Aspirin", &[("treats", "Headache")]),
        );
        write(
            &kb,
            "knowledge/disease/headache.md",
            &typed_page("Disease", "Headache", &[("has_phenotype", "Pain")]),
        );
        write(
            &kb,
            "knowledge/phenotype/pain.md",
            &typed_page("PhenotypicFeature", "Pain", &[("treated_by", "Aspirin")]),
        );

        let report = scan(&kb).unwrap();
        assert!(
            report.orphans.is_empty(),
            "every page has an inbound `edges:` entry, so none is an orphan — \
             this is the reported defect: {:?}",
            report.orphans
        );
    }

    /// …and the check still fires. A page with no inbound edge in **any**
    /// grammar is still an orphan, which is what separates "lint reads the typed
    /// edges now" from "lint stopped reporting orphans".
    #[test]
    fn a_page_with_no_inbound_edge_in_any_grammar_is_still_an_orphan() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write(
            &kb,
            "knowledge/molecule/aspirin.md",
            &typed_page("Molecule", "Aspirin", &[("treats", "Headache")]),
        );
        write(
            &kb,
            "knowledge/disease/headache.md",
            &typed_page("Disease", "Headache", &[("treated_by", "Aspirin")]),
        );
        // Points outward at Aspirin and is named by nobody.
        write(
            &kb,
            "knowledge/molecule/ibuprofen.md",
            &typed_page("Molecule", "Ibuprofen", &[("treats", "Headache")]),
        );

        let report = scan(&kb).unwrap();
        assert_eq!(
            report.orphans,
            vec!["knowledge/molecule/ibuprofen.md"],
            "an outbound-only page is still unreferenced"
        );
    }

    /// A base carrying both generations at once — one page reachable only by a
    /// typed `edges:` entry, one only by a legacy bracket link, one only by an
    /// OKF §6.1 markdown link. Each grammar is the *sole* inbound route to its
    /// page, so dropping any one of the three orphans exactly that page.
    #[test]
    fn a_mixed_base_reads_all_three_inbound_grammars() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write(
            &kb,
            "knowledge/molecule/aspirin.md",
            &typed_page("Molecule", "Aspirin", &[("treats", "Headache")]),
        );
        write(
            &kb,
            "knowledge/disease/headache.md",
            "---\ntype: Disease\nidentifier: Headache\n---\n\nSee also [[Migraine]].\n",
        );
        write(
            &kb,
            "knowledge/disease/migraine.md",
            "---\ntype: Disease\nidentifier: Migraine\n---\n\n\
             Related: [Cluster headache](/knowledge/disease/cluster-headache.md).\n",
        );
        write(
            &kb,
            "knowledge/disease/cluster-headache.md",
            &typed_page("Disease", "Cluster headache", &[("treated_by", "Aspirin")]),
        );

        let report = scan(&kb).unwrap();
        assert!(
            report.orphans.is_empty(),
            "one page per grammar, each with exactly one inbound link: {:?}",
            report.orphans
        );
    }

    /// `missing_concept_pages` had the same blindness, and it matters on the
    /// same bases: a typed source page cites its concepts through `edges:`, so a
    /// bracket-only reader found no citations at all and the list was silently
    /// empty on every base the migration produces.
    #[test]
    fn a_typed_source_pages_unresolved_citation_is_a_missing_concept_page() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write(
            &kb,
            "knowledge/disease/covid-19.md",
            &typed_page("Disease", "COVID-19", &[]),
        );
        write(
            &kb,
            "knowledge/sources/chen-2020.md",
            &typed_page(
                "Publication",
                "Chen 2020",
                &[("mentions", "COVID-19"), ("mentions", "Long COVID")],
            ),
        );

        let report = scan(&kb).unwrap();
        assert_eq!(
            report.missing_concept_pages,
            vec!["Long COVID".to_string()],
            "the cited concept with no page, and only it — `COVID-19` resolves, \
             so a lint that listed it would be resolving differently from the \
             edge the graph just drew"
        );
    }

    /// The graph and the lint must now agree by construction, because they read
    /// the same edge set. Asserted on a typed base, which is where they used to
    /// disagree most completely: 17 edges drawn, every page called an orphan.
    #[test]
    fn no_page_the_graph_draws_an_inbound_edge_to_is_reported_as_an_orphan() {
        let (_dir, svc) = svc_in(KbFormat::Biookf);
        let kb = svc.root().join("k");
        write(
            &kb,
            "knowledge/molecule/aspirin.md",
            &typed_page("Molecule", "Aspirin", &[("treats", "Headache")]),
        );
        write(
            &kb,
            "knowledge/disease/headache.md",
            &typed_page("Disease", "Headache", &[]),
        );

        let g = crate::knowledge::graph::derive(&kb).unwrap();
        let report = scan(&kb).unwrap();
        for edge in &g.edges {
            let Some(target) = g.nodes.iter().find(|n| n.id == edge.to) else {
                continue;
            };
            assert!(
                !report.orphans.contains(&target.path),
                "{} has an inbound {edge:?} in the graph and is an orphan in the \
                 lint; orphans={:?}",
                target.path,
                report.orphans
            );
        }
        assert!(!g.edges.is_empty(), "the premise of this test is gone");
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
            "page b is referenced by a, so not an orphan"
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
                caller_is_private: false,
                caller_affiliation: Default::default(),
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

    /// An autofix run that fixed nothing must not commit either.
    ///
    /// Every clean knowledge base used to gain an empty `[lint] lint autofix`
    /// entry from a `--fix` pass with nothing to do, because `commit_txn` will
    /// happily record a commit whose tree equals its parent's. The user sees it
    /// in the Knowledge change-log drawer as work that never happened, and the
    /// `commit_sha` invites callers to say so — issue #71's false success, one
    /// macro over.
    #[tokio::test]
    async fn an_autofix_that_fixed_nothing_does_not_commit() {
        use crate::knowledge::subagent::loop_::{Completer, LlmMessage, LlmReply};

        /// Reports there is nothing to fix, without calling a tool.
        struct NothingToFix;

        #[async_trait::async_trait]
        impl Completer for NothingToFix {
            async fn complete(
                &self,
                _system: &str,
                _messages: &[LlmMessage],
                _tools: &[rmcp::model::Tool],
            ) -> anyhow::Result<LlmReply> {
                Ok(LlmReply {
                    text: "Nothing needed fixing.".into(),
                    tool_calls: Vec::new(),
                })
            }
        }

        let (_dir, svc) = fresh_svc();
        let kb = svc.root().join("k");
        write_page(
            &kb,
            "knowledge/entities/a.md",
            "---\ntitle: A\nkind: entity\n---\n\nA body.",
            "add a",
            None,
        )
        .unwrap();

        let result = lint(
            &svc,
            LintArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                completer: Some(Box::new(NothingToFix)),
                autofix: true,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .unwrap();

        assert!(
            result.commit_sha.is_none(),
            "an autofix that changed nothing must not hand back a commit sha, got: {:?}",
            result.commit_sha
        );
        assert_eq!(result.fixes_applied, 0);

        let log = svc.list_history("k", 10).unwrap();
        assert!(
            !log.iter().any(|e| e.kind == ChangeKind::Lint),
            "no lint commit may appear in the change log; log: {log:?}"
        );
    }

    // ── Issue #56, Task 10B: CP2 ────────────────────────────────────────────

    /// The raise runs at the macro entry, before anything is scanned or fixed —
    /// so a lint that never reaches its autofix has still stamped the base.
    #[tokio::test]
    async fn the_lint_macro_ratchets_at_its_entry() {
        let (dir, svc) = fresh_svc();
        let root = dir.path().to_path_buf();
        assert!(!crate::knowledge::tier::is_private(&root, "k"));

        let _ = lint(
            &svc,
            LintArgs {
                kb_id: "k".into(),
                caller_is_private: true,
                caller_affiliation: Default::default(),
                completer: None,
                autofix: false,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await;

        assert!(
            crate::knowledge::tier::is_private(&root, "k"),
            "the lint macro did not raise at its entry"
        );
    }
}
