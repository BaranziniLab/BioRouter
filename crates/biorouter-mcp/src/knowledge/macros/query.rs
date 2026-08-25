//! `query` macro — runs a bounded sub-agent that searches the KB and synthesizes
//! an answer, optionally filing it as a new knowledge page.

use crate::knowledge::{
    git::{GitRepo, Txn},
    paths,
    service::KnowledgeService,
    subagent::{
        events::{DoneReason, SubAgentEvent},
        kb_tools::{read_only_tool_specs, tool_specs, KbToolAccess, KbToolDispatch},
        loop_::{Completer, SubAgent, SubAgentBounds},
        procedures::{query_procedure, system_prompt},
    },
    types::ChangeKind,
};
use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct QueryArgs {
    pub kb_id: String,
    /// The capability of the model this macro will run (issue #56). Required,
    /// so every production caller is a compile error rather than an omission.
    ///
    /// A query discloses the base to its model even when `file_as_page` is false,
    /// so the reachability barrier always applies. Only `file_as_page=true`
    /// writes model output back into the base and therefore raises its tier and
    /// affiliation.
    pub caller_is_private: bool,
    /// Whose agreements cover that model — DR-26's third axis (issue #56, Task
    /// 50). Required for the same reason `caller_is_private` is: an omission
    /// must be a compile error. `Unstated` is its `Default`, so a caller that
    /// cannot determine one fails closed.
    pub caller_affiliation: crate::knowledge::affiliation::CallerAffiliation,
    pub question: String,
    pub completer: Box<dyn Completer>,
    pub file_as_page: bool,
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
pub struct QueryResult {
    pub answer: String,
    pub cited_pages: Vec<String>,
    pub commit_sha: Option<String>,
}

/// Commit the query's transaction, but only if it actually filed a page.
///
/// `commit_txn` squash-commits the transaction *tree*, and git records a commit
/// whose tree equals its parent's without complaint, so a run that answered the
/// question but never called `kb_write_page` still produced a sha.
/// `biorouter knowledge query --save` prints "✓ saved as a page (<sha>)" off
/// exactly that value, and the page is not there — issue #71's false success,
/// one macro over.
///
/// Unlike an ingest this does not fail the call: the answer is the deliverable
/// and it is perfectly good. Only the claim that it was filed goes.
fn commit_txn_if_a_page_was_filed(
    svc: &KnowledgeService,
    kb_id: &str,
    kb_root: &std::path::Path,
    txn_branch: Option<String>,
    steps_used: usize,
) -> Result<Option<String>> {
    let Some(branch) = txn_branch else {
        return Ok(None);
    };
    let repo = GitRepo::open(kb_root)?;
    let txn = Txn { branch };
    let filed = match repo.txn_wrote_knowledge_pages(&txn) {
        Ok(filed) => filed,
        Err(e) => {
            let _ = repo.abort_txn(&txn);
            return Err(e.context("checking whether the query filed a page"));
        }
    };
    if !filed {
        let _ = repo.abort_txn(&txn);
        return Ok(None);
    }
    let sha = repo.commit_txn(
        &txn,
        ChangeKind::Query,
        "query filed",
        Some(&format!("+1 note · {steps_used} steps")),
    )?;
    svc.rebuild_graph_cache(kb_id)?;
    Ok(Some(sha))
}

fn abort_query_txn_if_open(kb_root: &std::path::Path, txn_branch: Option<&str>) -> Result<()> {
    let Some(branch) = txn_branch else {
        return Ok(());
    };
    let repo = GitRepo::open(kb_root)?;
    let txn = Txn {
        branch: branch.to_string(),
    };
    let _ = repo.abort_txn(&txn);
    Ok(())
}

pub async fn query(svc: &KnowledgeService, args: QueryArgs) -> Result<QueryResult> {
    let _lock = svc.lock_kb(&args.kb_id).await?;
    // Issue #56. Before the sub-agent, not after. Task 10C (CP2) puts the
    // barrier on the line above: a `query` reads the whole base into a model's
    // context, which is the disclosure this issue is about even when
    // `file_as_page` is false.
    crate::knowledge::tier::assert_reachable(
        svc.root(),
        &args.kb_id,
        args.caller_is_private,
        &args.caller_affiliation,
    )?;
    if args.file_as_page {
        // Issue #56, both axes in one call under one lock — see
        // `KnowledgeService::raise_tier_and_affiliation` for why they cannot be
        // two. A read-only query sends base content to the model but writes none
        // of that model's output back, so it must not permanently reclassify the
        // base merely because the model looked at it.
        svc.raise_tier_and_affiliation(
            &args.kb_id,
            args.caller_is_private,
            &args.caller_affiliation,
        )?;
    }
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    // Migrate a stale `schema.md` (the sub-agent's system prompt) and refresh a
    // stale graph cache, neither fatally. See `macros::refresh_base`.
    super::refresh_base(svc, &args.kb_id);

    // Open a txn only when we will commit a new note page.
    let txn_branch: Option<String> = if args.file_as_page {
        let repo = GitRepo::open(&kb_root)?;
        Some(repo.begin_txn("query")?.branch)
    } else {
        None
    };

    // Build the system prompt: schema.md + the profile's query procedure +
    // optional read-only reminder. `Manifest::profile` and never
    // `Manifest::format`, which reads `Okf` on every base written before Stage 3
    // (DR-6's trap, reached from the reader): a legacy base would then be taught
    // OKF's page contract and handed BioOKF's tools.
    let format = crate::knowledge::manifest::load(&kb_root)
        .ok()
        .and_then(|m| m.profile());
    let schema_path = crate::knowledge::store::resolve_readable_path(&kb_root, "schema.md")?;
    let schema = std::fs::read_to_string(schema_path).context("read schema.md")?;
    let mut system = system_prompt(&schema, query_procedure(format));
    if !args.file_as_page {
        system.push_str(
            "\n\nIMPORTANT: file_as_page is FALSE for this call. \
             Do NOT write any pages. Read-only.",
        );
    }

    let dispatch = KbToolDispatch {
        svc: svc.clone(),
        kb_id: args.kb_id.clone(),
        txn_branch: txn_branch.clone().unwrap_or_default(),
        access: if args.file_as_page {
            KbToolAccess::ReadWrite
        } else {
            KbToolAccess::ReadOnly
        },
    };
    let agent = SubAgent {
        completer: args.completer,
        tools: if args.file_as_page {
            tool_specs(format)
        } else {
            read_only_tool_specs(format)
        },
        system_prompt: system,
        bounds: args.bounds,
    };

    let cancel_ref = args.cancel.as_deref();
    let agent_result = agent
        .run(
            &args.question,
            std::sync::Arc::new(dispatch),
            cancel_ref,
            args.event_sink.as_ref(),
        )
        .await;

    match agent_result {
        Ok(r)
            if matches!(
                r.reason,
                DoneReason::CompleteSentinel | DoneReason::NoMoreToolCalls
            ) =>
        {
            // Commit the txn if we were filing the answer as a page — and only
            // if a page was actually filed. See the helper for why that
            // distinction matters.
            let commit_sha = commit_txn_if_a_page_was_filed(
                svc,
                &args.kb_id,
                &kb_root,
                txn_branch,
                r.steps_used,
            )?;

            Ok(QueryResult {
                answer: r.final_text,
                cited_pages: extract_wiki_links(&r.events),
                commit_sha,
            })
        }
        Ok(r) => {
            // Bad DoneReason: abort the txn branch if one was opened.
            abort_query_txn_if_open(&kb_root, txn_branch.as_deref())?;
            anyhow::bail!(
                "query sub-agent aborted: reason={:?}, final={}",
                r.reason,
                r.final_text
            )
        }
        Err(e) => {
            abort_query_txn_if_open(&kb_root, txn_branch.as_deref())?;
            Err(e)
        }
    }
}

/// Extract `[[Page Name]]` wiki-link references from all Step events in order,
/// deduplicating while preserving first-occurrence order.
fn extract_wiki_links(events: &[SubAgentEvent]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in events {
        if let SubAgentEvent::Step { assistant_text, .. } = e {
            for cited in extract_wiki_links_from_text(assistant_text) {
                if !out.contains(&cited) {
                    out.push(cited);
                }
            }
        }
    }
    out
}

/// The citations in one piece of assistant prose, in written order.
///
/// This used to be the third copy of the `[[…]]` regex, and the only one of the
/// three with **no** resolver at all: the raw capture went straight into
/// `cited_pages`, so `[[knowledge/entities/x|X]]` reached the user as the
/// citation `knowledge/entities/x|X`, alias and all. Reading the same grammar as
/// the graph and the lint (`knowledge::links`) is what makes the three
/// answerable against each other — and what stops BioOKF's inline edge sugar
/// from being cited as the literal string `treats:: COVID-19 | knowledge_level=…`
/// once pages start carrying it (DR-14).
///
/// Targets are still returned as the model wrote them, not resolved to pages:
/// a citation is prose, and there is no bundle in hand here. `pub(crate)` so the
/// equivalence test in `knowledge::links` can drive this consumer beside the
/// other two.
pub(crate) fn extract_wiki_links_from_text(text: &str) -> Vec<String> {
    crate::knowledge::links::wiki_links(text)
        .into_iter()
        .map(|link| link.target)
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{
        page_fixtures::valid_page,
        service::KnowledgeService,
        subagent::loop_::{LlmMessage, LlmReply, LlmToolCall},
        types::ChangeKind,
    };
    use async_trait::async_trait;
    use rmcp::model::Tool;
    use tokio::sync::Mutex;

    struct MockCompleter {
        replies: Mutex<Vec<LlmReply>>,
    }

    struct MutationProbeCompleter {
        step: Mutex<usize>,
    }

    impl MockCompleter {
        fn new(replies: Vec<LlmReply>) -> Self {
            Self {
                replies: Mutex::new(replies),
            }
        }
    }

    #[async_trait]
    impl Completer for MockCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            let mut q = self.replies.lock().await;
            if q.is_empty() {
                panic!("MockCompleter ran out of canned replies");
            }
            Ok(q.remove(0))
        }
    }

    #[async_trait]
    impl Completer for MutationProbeCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            let advertised: Vec<String> = tools.iter().map(|tool| tool.name.to_string()).collect();
            for mutation in [
                "kb_write_page",
                "kb_append_log",
                "kb_add_raw_source",
                "kb_write_concept",
            ] {
                assert!(
                    !advertised.iter().any(|name| name == mutation),
                    "read-only query advertised mutation {mutation}: {advertised:?}"
                );
            }

            let mut step = self.step.lock().await;
            let reply = match *step {
                0 => tool_call_reply(
                    "kb_write_page",
                    serde_json::json!({
                        "path": "knowledge/notes/forbidden.md",
                        "content": valid_page("note", "forbidden", "must not be written"),
                        "commit_message": "forbidden page"
                    }),
                ),
                1 => tool_call_reply(
                    "kb_append_log",
                    serde_json::json!({
                        "summary": "forbidden log entry",
                        "kind": "query"
                    }),
                ),
                2 => tool_call_reply(
                    "kb_add_raw_source",
                    serde_json::json!({
                        "type": "text",
                        "text": "forbidden raw source",
                        "title": "Forbidden"
                    }),
                ),
                3 => tool_call_reply(
                    "kb_write_concept",
                    serde_json::json!({
                        "type": "Gene",
                        "identifier": "FORBIDDEN"
                    }),
                ),
                4 => tool_call_reply(
                    "kb_delete_page",
                    serde_json::json!({ "path": "knowledge/entities/hrv.md" }),
                ),
                _ => text_reply_with_citation("The attempted mutations were refused [[HRV]]."),
            };
            *step += 1;
            Ok(reply)
        }
    }

    fn tool_call_reply(tool_name: &str, args: serde_json::Value) -> LlmReply {
        LlmReply {
            text: String::new(),
            tool_calls: vec![LlmToolCall {
                id: "req-1".into(),
                name: tool_name.to_string(),
                args,
            }],
        }
    }

    fn text_reply_with_citation(text: &str) -> LlmReply {
        LlmReply {
            text: text.to_string(),
            tool_calls: vec![],
        }
    }

    fn fresh_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir, svc)
    }

    fn git_state(kb_root: &std::path::Path) -> (String, String, Vec<String>) {
        let repo = git2::Repository::open(kb_root).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let head_id = head.id().to_string();
        let tree_id = head.tree_id().to_string();
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap();
        let history = walk.map(|oid| oid.unwrap().to_string()).collect::<Vec<_>>();
        (head_id, tree_id, history)
    }

    fn directory_state(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
        fn visit(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
            if !dir.exists() {
                return;
            }
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(base, &path, out);
                } else {
                    out.push((
                        path.strip_prefix(base)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        std::fs::read(path).unwrap(),
                    ));
                }
            }
        }

        let mut out = Vec::new();
        visit(root, root, &mut out);
        out.sort_by(|left, right| left.0.cmp(&right.0));
        out
    }

    // -------------------------------------------------------------------------
    // Test 1: read-only query returns answer with citations, no commit
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn query_read_only_returns_answer_with_citations() {
        let (_dir, svc) = fresh_svc();

        // Write a fixture page so kb_search actually finds it.
        let kb = svc.root().join("k");
        crate::knowledge::store::write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntitle: HRV\nkind: entity\n---\n\nHeart rate variability is a key marker.",
            "fixture",
            None,
        )
        .unwrap();

        // step 0: tool call to kb_search
        // step 1: final text reply with a wiki-link citation
        let completer = MockCompleter::new(vec![
            tool_call_reply("kb_search", serde_json::json!({ "query": "HRV" })),
            text_reply_with_citation("Heart rate variability [[HRV]] is improved by zone-2."),
        ]);

        let result = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                question: "How does zone-2 affect HRV?".into(),
                completer: Box::new(completer),
                file_as_page: false,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .unwrap();

        assert!(
            result.cited_pages.contains(&"HRV".to_string()),
            "cited_pages should contain 'HRV', got: {:?}",
            result.cited_pages
        );
        assert!(
            result.commit_sha.is_none(),
            "read-only query must not commit"
        );
    }

    #[tokio::test]
    async fn read_only_query_refuses_advertised_and_hallucinated_mutations_without_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_in("k", "K", None, crate::knowledge::types::KbFormat::Biookf)
            .unwrap();
        let kb = svc.root().join("k");
        crate::knowledge::store::write_page(
            &kb,
            "knowledge/entities/hrv.md",
            "---\ntype: Gene\nidentifier: HRV\n---\n\nseed page",
            "seed",
            None,
        )
        .unwrap();

        let repo_before = git_state(&kb);
        let pages_before = directory_state(&kb.join("knowledge"));
        let raw_before = directory_state(&kb.join("raw"));
        let log_before = std::fs::read(kb.join("log.md")).unwrap();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

        let result = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                question: "Try to mutate this base.".into(),
                completer: Box::new(MutationProbeCompleter {
                    step: Mutex::new(0),
                }),
                file_as_page: false,
                bounds: SubAgentBounds::default(),
                event_sink: Some(event_tx),
                cancel: None,
            },
        )
        .await
        .unwrap();

        assert!(result.commit_sha.is_none());
        let mut rejected = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            if let SubAgentEvent::ToolResult {
                name, ok: false, ..
            } = event
            {
                rejected.push(name);
            }
        }
        assert_eq!(
            rejected,
            vec![
                "kb_write_page".to_string(),
                "kb_append_log".to_string(),
                "kb_add_raw_source".to_string(),
                "kb_write_concept".to_string(),
                "kb_delete_page".to_string(),
            ]
        );
        assert_eq!(git_state(&kb).0, repo_before.0, "HEAD changed");
        assert_eq!(git_state(&kb).1, repo_before.1, "HEAD tree changed");
        assert_eq!(git_state(&kb).2, repo_before.2, "history changed");
        assert_eq!(
            directory_state(&kb.join("knowledge")),
            pages_before,
            "knowledge pages changed"
        );
        assert_eq!(
            directory_state(&kb.join("raw")),
            raw_before,
            "raw sources changed"
        );
        assert_eq!(
            std::fs::read(kb.join("log.md")).unwrap(),
            log_before,
            "knowledge log changed"
        );
    }

    // -------------------------------------------------------------------------
    // Test 2: file_as_page=true — agent writes a note and commits
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn query_with_file_as_page_creates_note_and_commits() {
        let (_dir, svc) = fresh_svc();

        // step 0: write the note page
        // step 1: final text reply (done)
        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_page",
                serde_json::json!({
                    "path": "knowledge/notes/zone2-hrv.md",
                    "content": valid_page("note", "zone2-hrv", "Answer."),
                    "commit_message": "file query"
                }),
            ),
            text_reply_with_citation("Answer [[HRV]]."),
        ]);

        let result = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                question: "What is zone-2 HRV effect?".into(),
                completer: Box::new(completer),
                file_as_page: true,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .unwrap();

        assert!(
            result.commit_sha.is_some(),
            "file_as_page=true must produce a commit"
        );

        // The note page should exist on main after the commit.
        let note_path = svc.root().join("k").join("knowledge/notes/zone2-hrv.md");
        assert!(
            note_path.exists(),
            "note page must exist after commit; path: {}",
            note_path.display()
        );

        assert!(
            result.cited_pages.contains(&"HRV".to_string()),
            "citations should be extracted: {:?}",
            result.cited_pages
        );
    }

    // -------------------------------------------------------------------------
    // Test 2b: file_as_page=true but the run filed nothing → no commit sha
    //
    // The same false success as issue #71, one macro over. `commit_txn` squash
    // commits the transaction *tree*, and git records a commit whose tree equals
    // its parent's without complaint — so a run that answered the question but
    // never wrote the note still produced a sha. `biorouter knowledge query
    // --save` prints "✓ saved as a page (<sha>)" off exactly that value, and the
    // page is not there.
    //
    // Unlike an ingest, this does not fail the call: the answer is the
    // deliverable and it is perfectly good. Only the claim that it was filed has
    // to go.
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn a_query_that_filed_no_page_reports_no_commit() {
        let (_dir, svc) = fresh_svc();

        // The model answers in prose and never calls kb_write_page.
        let completer = MockCompleter::new(vec![text_reply_with_citation(
            "Zone-2 raises HRV over weeks [[HRV]].",
        )]);

        let result = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                question: "What is the zone-2 HRV effect?".into(),
                completer: Box::new(completer),
                file_as_page: true,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect("the answer itself is still good and must be returned");

        assert!(
            result.commit_sha.is_none(),
            "a query that filed no page must not hand back a commit sha, got: {:?}",
            result.commit_sha
        );
        assert!(
            result.answer.contains("Zone-2"),
            "the answer must survive the missing page, got: {}",
            result.answer
        );

        let notes = svc.root().join("k").join("knowledge/notes");
        assert!(
            std::fs::read_dir(&notes)
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
            "no note page may exist when none was filed"
        );
    }

    // -------------------------------------------------------------------------
    // Test 3: step budget exceeded with file_as_page=true → txn aborted
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn query_aborts_txn_on_step_budget() {
        let (_dir, svc) = fresh_svc();

        // Always return a kb_search tool call so the loop never terminates naturally.
        let replies: Vec<LlmReply> = (0..20)
            .map(|_| tool_call_reply("kb_search", serde_json::json!({ "query": "HRV" })))
            .collect();
        let completer = MockCompleter::new(replies);

        let err = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                question: "What is HRV?".into(),
                completer: Box::new(completer),
                file_as_page: true,
                bounds: SubAgentBounds {
                    max_steps: 2,
                    ..Default::default()
                },
                event_sink: None,
                cancel: None,
            },
        )
        .await;

        assert!(err.is_err(), "query should fail when step budget exceeded");

        // The KB git history must NOT contain a query-kind commit with 'steps' in
        // the delta — that would only appear if commit_txn was called.
        let log = svc.list_history("k", 10).unwrap();
        let has_query_commit = log.iter().any(|e| {
            e.kind == ChangeKind::Query
                && e.delta
                    .as_deref()
                    .map(|d| d.contains("steps"))
                    .unwrap_or(false)
        });
        assert!(
            !has_query_commit,
            "no query commit should exist after step-budget abort; log: {log:?}"
        );
    }

    // ── Issue #56, Task 10B: CP2 ────────────────────────────────────────────

    /// A completer that never answers, so the sub-agent run fails immediately.
    /// The test below says nothing about the macro's happy path and everything
    /// about WHEN the raise runs.
    struct RefusesImmediately;

    #[async_trait]
    impl Completer for RefusesImmediately {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            anyhow::bail!("no model here")
        }
    }

    struct PanicsIfCalled;

    #[async_trait]
    impl Completer for PanicsIfCalled {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            panic!("an unreachable base must be rejected before invoking the model")
        }
    }

    /// The read-only boundary is not only a tool filter: merely running the
    /// query through a private institutional model must not stamp a public base
    /// private or claim it for that institution. The reachability check still
    /// runs before the model is called.
    #[tokio::test]
    async fn a_read_only_query_does_not_ratchet_tier_or_affiliation() {
        let (dir, svc) = fresh_svc();
        let root = dir.path().to_path_buf();
        assert!(!crate::knowledge::tier::is_private(&root, "k"));

        let _ = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: true,
                caller_affiliation: crate::knowledge::affiliation::CallerAffiliation::Institution(
                    "fixture-institution".to_string(),
                ),
                question: "what is n?".into(),
                completer: Box::new(RefusesImmediately),
                file_as_page: false,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await;

        assert!(
            !crate::knowledge::tier::is_private(&root, "k"),
            "a read-only query permanently privatised a public base"
        );
        assert_eq!(
            crate::knowledge::tier::affiliation(&root, "k"),
            crate::knowledge::affiliation::KbAffiliation::Owners(Default::default()),
            "a read-only query permanently claimed a public base for its model's institution"
        );
    }

    #[tokio::test]
    async fn a_read_only_query_still_enforces_reachability_before_the_model() {
        let (dir, svc) = fresh_svc();
        crate::knowledge::tier::raise_unlocked(dir.path(), "k", true).unwrap();

        let error = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                question: "what is n?".into(),
                completer: Box::new(PanicsIfCalled),
                file_as_page: false,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("a public model must not read a private base");

        assert!(error.to_string().contains("private"), "{error:#}");
    }
}
