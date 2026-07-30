//! `query` macro — runs a bounded sub-agent that searches the KB and synthesizes
//! an answer, optionally filing it as a new knowledge page.

use crate::knowledge::{
    git::{GitRepo, Txn},
    paths,
    service::KnowledgeService,
    subagent::{
        events::{DoneReason, SubAgentEvent},
        kb_tools::{tool_specs, KbToolDispatch},
        loop_::{Completer, SubAgent, SubAgentBounds},
        procedures::QUERY_PROCEDURE,
    },
    types::ChangeKind,
};
use anyhow::{Context, Result};
use regex::Regex;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct QueryArgs {
    pub kb_id: String,
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

pub async fn query(svc: &KnowledgeService, args: QueryArgs) -> Result<QueryResult> {
    let _lock = svc.lock_kb(&args.kb_id).await?;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    // Idempotently upgrade legacy schema.md files that pre-date the
    // cross-reference rules section. No-op for already-migrated KBs.
    let _ = svc.migrate_schema_if_needed(&args.kb_id);
    // Refresh the graph cache so any stale 0-edge cache produced by the
    // pre-fix wiki-link deriver is replaced with a freshly derived one.
    let _ = svc.rebuild_graph_cache(&args.kb_id);

    // Open a txn only when we will commit a new note page.
    let txn_branch: Option<String> = if args.file_as_page {
        let repo = GitRepo::open(&kb_root)?;
        Some(repo.begin_txn("query")?.branch)
    } else {
        None
    };

    // Build the system prompt: schema.md + QUERY_PROCEDURE + optional read-only reminder.
    let schema = std::fs::read_to_string(kb_root.join("schema.md")).context("read schema.md")?;
    let mut system = format!("{schema}\n\n---\n{QUERY_PROCEDURE}");
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
    };
    let agent = SubAgent {
        completer: args.completer,
        tools: tool_specs(),
        system_prompt: system,
        bounds: args.bounds,
    };

    let cancel_ref = args.cancel.as_deref();
    let agent_result = agent
        .run(
            &args.question,
            &dispatch,
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
            // if a page was actually filed.
            //
            // `commit_txn` squash-commits the transaction *tree*, and git records
            // a commit whose tree equals its parent's without complaint, so a run
            // that answered the question but never called `kb_write_page` still
            // produced a sha. `biorouter knowledge query --save` prints
            // "✓ saved as a page (<sha>)" off exactly that value, and the page is
            // not there — issue #71's false success, one macro over.
            //
            // Unlike an ingest this does not fail the call: the answer is the
            // deliverable and it is perfectly good. Only the claim that it was
            // filed goes.
            let commit_sha = if let Some(branch) = txn_branch {
                let repo = GitRepo::open(&kb_root)?;
                let txn = Txn { branch };
                let filed = match repo.txn_wrote_knowledge_pages(&txn) {
                    Ok(filed) => filed,
                    Err(e) => {
                        let _ = repo.abort_txn(&txn);
                        return Err(e.context("checking whether the query filed a page"));
                    }
                };
                if filed {
                    let sha = repo.commit_txn(
                        &txn,
                        ChangeKind::Query,
                        "query filed",
                        Some(&format!("+1 note · {} steps", r.steps_used)),
                    )?;
                    svc.rebuild_graph_cache(&args.kb_id)?;
                    Some(sha)
                } else {
                    let _ = repo.abort_txn(&txn);
                    None
                }
            } else {
                None
            };

            Ok(QueryResult {
                answer: r.final_text,
                cited_pages: extract_wiki_links(&r.events),
                commit_sha,
            })
        }
        Ok(r) => {
            // Bad DoneReason: abort the txn branch if one was opened.
            if let Some(branch) = &txn_branch {
                let repo = GitRepo::open(&kb_root)?;
                let txn = Txn {
                    branch: branch.clone(),
                };
                let _ = repo.abort_txn(&txn);
            }
            anyhow::bail!(
                "query sub-agent aborted: reason={:?}, final={}",
                r.reason,
                r.final_text
            )
        }
        Err(e) => {
            if let Some(branch) = &txn_branch {
                let repo = GitRepo::open(&kb_root)?;
                let txn = Txn {
                    branch: branch.clone(),
                };
                let _ = repo.abort_txn(&txn);
            }
            Err(e)
        }
    }
}

/// Extract `[[Page Name]]` wiki-link references from all Step events in order,
/// deduplicating while preserving first-occurrence order.
fn extract_wiki_links(events: &[SubAgentEvent]) -> Vec<String> {
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let mut out: Vec<String> = Vec::new();
    for e in events {
        if let SubAgentEvent::Step { assistant_text, .. } = e {
            for cap in re.captures_iter(assistant_text) {
                let s = cap.get(1).unwrap().as_str().to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{
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
                    "content": "---\ntitle: zone2-hrv\nkind: note\n---\n\nAnswer.",
                    "commit_message": "file query"
                }),
            ),
            text_reply_with_citation("Answer [[HRV]]."),
        ]);

        let result = query(
            &svc,
            QueryArgs {
                kb_id: "k".into(),
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
}
