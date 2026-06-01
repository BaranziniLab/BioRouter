//! `ingest` macro — wraps the sub-agent loop to integrate a new source into
//! a KB with a single, txn-atomic git commit.

use crate::knowledge::{
    convert::SourceInput,
    git::GitRepo,
    paths,
    service::KnowledgeService,
    subagent::{
        events::{DoneReason, SubAgentEvent},
        kb_tools::{tool_specs, KbToolDispatch},
        loop_::{Completer, SubAgent, SubAgentBounds},
        procedures::INGEST_PROCEDURE,
    },
    types::ChangeKind,
};
use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct IngestArgs {
    pub kb_id: String,
    pub source: SourceInput,
    pub completer: Box<dyn Completer>,
    pub focus: Option<String>,
    pub bounds: SubAgentBounds,
    /// Optional channel for live event streaming. When provided, every
    /// `SubAgentEvent` is sent here as soon as it is produced (not just at
    /// the end of the run). Set to `None` if streaming is not needed.
    pub event_sink: Option<tokio::sync::mpsc::UnboundedSender<SubAgentEvent>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IngestResult {
    pub source_id: String,
    pub commit_sha: String,
    pub steps: usize,
    pub events: Vec<SubAgentEvent>,
}

pub async fn ingest(svc: &KnowledgeService, args: IngestArgs) -> Result<IngestResult> {
    let _lock = svc.lock_kb(&args.kb_id).await;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    // Materialize the raw source outside the sub-agent txn so it is durable
    // even if the sub-agent fails.
    let raw = svc
        .add_raw_source(&args.kb_id, args.source, None)
        .await
        .context("add_raw_source")?;

    // Open a transaction branch for the wiki-integration work.
    let repo = GitRepo::open(&kb_root)?;
    let txn = repo.begin_txn(&format!("ingest-{}", raw.source_id))?;

    // Build the system prompt: schema.md + INGEST_PROCEDURE.
    let schema = std::fs::read_to_string(kb_root.join("schema.md")).context("read schema.md")?;
    let system = format!("{schema}\n\n---\n{INGEST_PROCEDURE}");

    let dispatch = KbToolDispatch {
        svc: svc.clone(),
        kb_id: args.kb_id.clone(),
        txn_branch: txn.branch.clone(),
    };
    let agent = SubAgent {
        completer: args.completer,
        tools: tool_specs(),
        system_prompt: system,
        bounds: args.bounds,
    };
    let focus_line = args.focus.as_deref().unwrap_or("");
    let user = format!(
        "New source to integrate: source-id={}. Focus hints: {focus_line}",
        raw.source_id
    );

    let agent_result = agent
        .run(&user, &dispatch, None, args.event_sink.as_ref())
        .await;

    match agent_result {
        Ok(r)
            if matches!(
                r.reason,
                DoneReason::CompleteSentinel | DoneReason::NoMoreToolCalls
            ) =>
        {
            let sha = repo.commit_txn(
                &txn,
                ChangeKind::Ingest,
                &format!("ingest {}", raw.source_id),
                Some(&format!("+1 source · {} steps", r.steps_used)),
            )?;
            Ok(IngestResult {
                source_id: raw.source_id,
                commit_sha: sha,
                steps: r.steps_used,
                events: r.events,
            })
        }
        Ok(r) => {
            let _ = repo.abort_txn(&txn);
            anyhow::bail!(
                "ingest sub-agent aborted: reason={:?}, final={}",
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
    use crate::knowledge::{
        git::GitRepo,
        service::KnowledgeService,
        subagent::loop_::{LlmReply, LlmToolCall},
        types::ChangeKind,
    };
    use async_trait::async_trait;
    use rmcp::model::Tool;
    use tokio::sync::Mutex;

    // -------------------------------------------------------------------------
    // Minimal MockCompleter — pops canned replies from a queue.
    // -------------------------------------------------------------------------

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
            _messages: &[crate::knowledge::subagent::loop_::LlmMessage],
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

    fn text_reply(text: &str) -> LlmReply {
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
    // Test 1: happy path — agent writes a page and returns "done"
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn ingest_writes_pages_and_commits_one_change() {
        let (_dir, svc) = fresh_svc();

        // The mock completer:
        //   step 0: calls kb_write_page with a stub source page
        //   step 1: returns a text-only reply ("done") → NoMoreToolCalls
        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_page",
                serde_json::json!({
                    "path": "knowledge/sources/stub.md",
                    "content": "---\ntitle: stub\nkind: source\n---\n\nStub.",
                    "commit_message": "add source"
                }),
            ),
            text_reply("done"),
        ]);

        let result = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
            },
        )
        .await
        .unwrap();

        // 1 iteration of tool-call + 1 step with text reply = steps_used 1
        assert_eq!(result.steps, 1);
        assert!(!result.commit_sha.is_empty());

        // The ingest commit must appear as the latest history entry.
        let log = svc.list_history("k", 10).unwrap();
        // Expected: create + add_raw + squashed-ingest = 3
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].kind, ChangeKind::Ingest);

        // The source page written by the sub-agent must be on main.
        let kb = svc.root().join("k");
        assert!(
            kb.join("knowledge/sources/stub.md").exists(),
            "source page must exist after commit"
        );
    }

    // -------------------------------------------------------------------------
    // Test 2: step budget exceeded → txn aborted, git history unchanged
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn ingest_aborts_txn_on_step_budget() {
        let (_dir, svc) = fresh_svc();

        // Always return a tool call so the loop never ends.
        let replies: Vec<LlmReply> = (0..20)
            .map(|_| tool_call_reply("kb_list_pages", serde_json::json!({})))
            .collect();
        let completer = MockCompleter::new(replies);

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                source: SourceInput::Text {
                    text: "Some note.".into(),
                    title: Some("x".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds {
                    max_steps: 3,
                    ..Default::default()
                },
                event_sink: None,
            },
        )
        .await;

        assert!(err.is_err(), "should fail when step budget exceeded");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.to_lowercase().contains("step")
                || msg.to_lowercase().contains("budget")
                || msg.to_lowercase().contains("aborted"),
            "error should mention budget or abort, got: {msg}"
        );

        // The KB history must not have a macro-level Ingest commit (the squash-merge).
        // add_raw_source itself commits with ChangeKind::Ingest and summary "ingested <id>",
        // which is expected. The macro's squash-merge would produce summary "ingest <id>"
        // (no "d" suffix). We check that no such commit exists.
        let log = svc.list_history("k", 10).unwrap();
        let has_macro_ingest_commit = log.iter().any(|e| {
            e.kind == ChangeKind::Ingest
                && e.delta
                    .as_deref()
                    .map(|d| d.contains("steps"))
                    .unwrap_or(false)
        });
        assert!(
            !has_macro_ingest_commit,
            "no macro ingest commit (with 'steps' delta) should appear after budget exceeded; log: {log:?}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 3: dispatch fails with invalid path → KB not corrupted
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn ingest_aborts_txn_when_dispatch_fails() {
        let (_dir, svc) = fresh_svc();

        // Step 0: agent tries to escape the KB via path traversal.
        // The dispatch will return an error; the loop records a ToolResult(ok:false)
        // and pushes an error string back.  Then step 1 finishes with a text reply.
        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_page",
                serde_json::json!({
                    "path": "knowledge/../escape.md",
                    "content": "evil",
                    "commit_message": "escape"
                }),
            ),
            text_reply("recovered"),
        ]);

        // The macro might succeed (agent recovered after the error) or fail — both
        // are valid.  We only assert the KB was not corrupted.
        let kb = svc.root().join("k");
        let _ = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                source: SourceInput::Text {
                    text: "Some note.".into(),
                    title: Some("y".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
            },
        )
        .await;

        // The escape path must not exist.
        assert!(
            !kb.join("escape.md").exists(),
            "path traversal must not write outside knowledge/"
        );
        // The git repo must still be openable (not corrupted).
        GitRepo::open(&kb).unwrap();
    }
}
