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
        loop_::{Completer, SubAgent, SubAgentBounds, SubAgentResult},
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
    /// The capability of the model this macro will run (issue #56). Required,
    /// so all four production callers are a compile error rather than an
    /// omission. A `bool` and not `ProviderTier` because `biorouter-mcp` cannot
    /// depend on `biorouter`, where that enum lives.
    pub caller_is_private: bool,
    pub source: SourceInput,
    pub completer: Box<dyn Completer>,
    pub focus: Option<String>,
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
pub struct IngestResult {
    pub source_id: String,
    pub commit_sha: String,
    pub steps: usize,
    pub events: Vec<SubAgentEvent>,
}

pub async fn ingest(svc: &KnowledgeService, args: IngestArgs) -> Result<IngestResult> {
    let _lock = svc.lock_kb(&args.kb_id).await?;
    // Issue #56. The ratchet for EVERY sub-agent macro, because `KbToolDispatch`
    // (subagent/kb_tools.rs) is bound to this one `kb_id` and reaches `store::*`
    // directly — there is no lower seam, and no MCP gate can see it. Before the
    // sub-agent, not after: a run that fails halfway has already written pages.
    // Task 10C adds `tier::assert_reachable(..)` on the line above.
    svc.raise_tier(&args.kb_id, args.caller_is_private)?;
    let kb_root = paths::kb_root(svc.root(), &args.kb_id);

    // Idempotently upgrade legacy schema.md files that pre-date the
    // cross-reference rules section. No-op for already-migrated KBs.
    let _ = svc.migrate_schema_if_needed(&args.kb_id);
    // Refresh the graph cache so any stale 0-edge cache produced by the
    // pre-fix wiki-link deriver is replaced with a freshly derived one.
    let _ = svc.rebuild_graph_cache(&args.kb_id);

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
            // A clean exit is not the same as a digest. `INGEST_PROCEDURE`
            // requires the run to write `knowledge/sources/<source-id>.md`, so a
            // transaction that left `knowledge/` exactly as it found it produced
            // no knowledge — and committing it would hand the caller a commit sha
            // for work that never happened (issue #71). The most common cause is
            // a provider that failed mid-request: the turn comes back as a bare
            // apology, or (Google, candidate with no `parts`) as a wholly empty
            // message, and both look exactly like "the agent has no more tool
            // calls". It is `knowledge/` and not the whole tree because the
            // procedure's other steps — the `raw/` source, the `index.md` entry,
            // the `log.md` line — each move the tree without adding knowledge, so
            // a run cut short after one of them would otherwise pass.
            //
            // A failure to *answer* the question aborts too: leaving HEAD parked
            // on the txn branch is how the next write to this KB lands somewhere
            // nobody is looking.
            let wrote_knowledge = match repo.txn_wrote_knowledge_pages(&txn) {
                Ok(changed) => changed,
                Err(e) => {
                    let _ = repo.abort_txn(&txn);
                    return Err(e.context("checking whether the ingest wrote anything"));
                }
            };
            if !wrote_knowledge {
                let _ = repo.abort_txn(&txn);
                anyhow::bail!(no_pages_written_error(&raw.source_id, &r));
            }
            let sha = repo.commit_txn(
                &txn,
                ChangeKind::Ingest,
                &format!("ingest {}", raw.source_id),
                Some(&format!("+1 source · {} steps", r.steps_used)),
            )?;
            svc.rebuild_graph_cache(&args.kb_id)?;
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

/// The message a digest that wrote nothing fails with.
///
/// It has to answer the only question the user has — *what failed* — so it
/// carries the model's own last words and every tool call that errored. Without
/// them a silent model produces a silent failure, which is barely better than
/// the false success it replaces.
fn no_pages_written_error(source_id: &str, result: &SubAgentResult) -> String {
    let mut msg = format!(
        "ingest wrote no knowledge pages for source {source_id} \
         ({} step(s), ended: {:?}). Nothing was added to the knowledge base",
        result.steps_used, result.reason
    );

    let failures: Vec<String> = result
        .events
        .iter()
        .filter_map(|e| match e {
            SubAgentEvent::ToolResult {
                name,
                ok: false,
                summary,
            } => Some(format!("{name}: {}", clip(summary, 160))),
            _ => None,
        })
        .rev()
        .take(3)
        .collect();
    if !failures.is_empty() {
        msg.push_str(&format!("; failed tool calls: {}", failures.join(" | ")));
    }

    let final_text = result.final_text.trim();
    if final_text.is_empty() {
        msg.push_str(
            "; the model returned no final message, which usually means the \
             provider request failed or was cut short",
        );
    } else {
        msg.push_str(&format!(
            "; the model's last message was: {}",
            clip(final_text, 400)
        ));
    }
    msg
}

/// Keep the failure message readable: it travels through an SSE frame and lands
/// in a single line of the digest panel, where a few thousand characters of
/// model output would bury the part that explains what happened.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
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
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
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
                caller_is_private: false,
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
                cancel: None,
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
    // Test 2b: the sub-agent ended cleanly but wrote nothing → must NOT succeed
    //
    // Issue #71: a provider that fails mid-request commonly ends the turn with a
    // plain sentence and no tool calls, and Google's decoder returns a *content
    // free* assistant message when a candidate carries no parts (finishReason
    // MAX_TOKENS / SAFETY). Both land on `DoneReason::NoMoreToolCalls`, which the
    // macro read as "the agent is finished" — squash-committing an unchanged tree
    // and handing the caller a commit sha. The digest reported "completed" while
    // the KB gained no knowledge page at all.
    // -------------------------------------------------------------------------

    /// The reply carries an error sentence and no tool calls.
    #[tokio::test]
    async fn ingest_fails_when_the_subagent_writes_nothing() {
        let (_dir, svc) = fresh_svc();

        let completer = MockCompleter::new(vec![text_reply(
            "I'm sorry - the model provider returned an error and I cannot continue.",
        )]);

        let result = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await;

        let err = result
            .expect_err("a digest that wrote no knowledge page must not report success")
            .to_string();
        assert!(
            err.contains("no knowledge"),
            "the error must say the digest wrote nothing, got: {err}"
        );
        assert!(
            err.contains("model provider returned an error"),
            "the error must carry the model's own last words so the user learns \
             what failed, got: {err}"
        );

        // Nothing may be committed on top of the raw source.
        let log = svc.list_history("k", 10).unwrap();
        assert!(
            !log.iter().any(|e| e
                .delta
                .as_deref()
                .map(|d| d.contains("steps"))
                .unwrap_or(false)),
            "no macro ingest commit may exist after an empty digest; log: {log:?}"
        );
    }

    /// The provider hands back a completely empty assistant message — no text and
    /// no tool calls. Google produces exactly this when the candidate has no
    /// `parts` (thinking budget exhausted, safety stop).
    #[tokio::test]
    async fn ingest_fails_when_the_provider_returns_an_empty_reply() {
        let (_dir, svc) = fresh_svc();

        let completer = MockCompleter::new(vec![text_reply("")]);

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("an empty provider reply must not report a completed digest")
        .to_string();

        assert!(
            err.contains("no knowledge"),
            "the error must say the digest wrote nothing, got: {err}"
        );
    }

    /// Every tool call failed, then the model gave up with a text reply. The
    /// error must name the tool failures — "what failed" is the whole point.
    #[tokio::test]
    async fn ingest_failure_names_the_failed_tool_calls() {
        let (_dir, svc) = fresh_svc();

        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_page",
                serde_json::json!({
                    "path": "knowledge/../escape.md",
                    "content": "evil",
                    "commit_message": "escape"
                }),
            ),
            text_reply("I could not write the page."),
        ]);

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("a digest whose every tool call failed must not report success")
        .to_string();

        assert!(
            err.contains("kb_write_page"),
            "the error must name the tool that failed, got: {err}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 2c: the run touched the transaction, but never wrote knowledge
    //
    // "The tree changed" is not the same as "a digest happened". Three of the
    // sub-agent's tools write outside `knowledge/`: `kb_append_log` appends to
    // `log.md`, `kb_add_raw_source` materialises under `raw/`, and `kb_write_page`
    // accepts the top-level `index.md`. Each commits on the transaction branch,
    // so each on its own moves the txn tree away from main's while the KB gains
    // no knowledge at all.
    //
    // This is not a hypothetical: `INGEST_PROCEDURE` asks for the source page
    // (step 4), the index update (step 9) and the log line (step 10), so a
    // provider that dies part-way through — the reported failure — routinely
    // leaves exactly this state behind.
    // -------------------------------------------------------------------------

    /// Only `log.md` was touched: the run announced a digest it never performed.
    #[tokio::test]
    async fn ingest_fails_when_the_run_only_appended_a_log_line() {
        let (_dir, svc) = fresh_svc();

        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_append_log",
                serde_json::json!({ "summary": "digested the HRV note", "kind": "ingest" }),
            ),
            text_reply("The provider failed before I could write the page."),
        ]);

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("a log line is not a digest and must not report success")
        .to_string();

        assert!(
            err.contains("no knowledge"),
            "the error must say the digest wrote no knowledge, got: {err}"
        );

        // The claim has to hold on disk: nothing under knowledge/.
        let pages = crate::knowledge::store::list_pages(
            &paths::kb_root(svc.root(), "k"),
            Some("knowledge/"),
        )
        .unwrap();
        assert!(
            pages.is_empty(),
            "no knowledge page may exist after a log-only run; pages: {pages:?}"
        );
    }

    /// Only `raw/` was touched: a second source was materialised and nothing was
    /// ever integrated into the wiki.
    #[tokio::test]
    async fn ingest_fails_when_the_run_only_added_another_raw_source() {
        let (_dir, svc) = fresh_svc();

        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_add_raw_source",
                serde_json::json!({ "type": "text", "text": "A second note.", "title": "Second" }),
            ),
            text_reply("I stashed the source but could not integrate it."),
        ]);

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("stashing a raw source is not a digest and must not report success")
        .to_string();

        assert!(
            err.contains("no knowledge"),
            "the error must say the digest wrote no knowledge, got: {err}"
        );
    }

    // -------------------------------------------------------------------------
    // Test 2d: the provider itself fails mid-run
    //
    // `ProviderCompleter` now *fails* a completion when the provider hands back a
    // tool call Biorouter cannot decode, rather than dropping it and letting the
    // run look finished. That turns a provider error into an `Err` travelling out
    // of the sub-agent loop — a route through this macro that nothing exercised
    // before, and the one the reported failure takes.
    //
    // What it must leave behind is a usable knowledge base. `abort_txn` moves HEAD
    // back to main and deletes the transaction branch; if it ever stopped doing
    // so, every later write to this KB would commit onto a branch nobody reads
    // while the Knowledge view kept reading main. The user would digest the next
    // source "successfully" and still see nothing — issue #70's symptom, arrived
    // at from the other end.
    // -------------------------------------------------------------------------

    /// Fails once the canned replies run out, the way `ProviderCompleter` does.
    struct FailsWhenRepliesRunOut {
        replies: Mutex<Vec<LlmReply>>,
    }

    #[async_trait]
    impl Completer for FailsWhenRepliesRunOut {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[crate::knowledge::subagent::loop_::LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            let mut q = self.replies.lock().await;
            match q.is_empty() {
                true => anyhow::bail!(
                    "the model requested a tool call Biorouter could not decode: \
                     The provided function name 'kb.write page' had invalid characters"
                ),
                false => Ok(q.remove(0)),
            }
        }
    }

    #[tokio::test]
    async fn a_provider_error_aborts_the_txn_and_leaves_head_on_main() {
        let (dir, svc) = fresh_svc();

        // The run gets one real page written before the provider dies, so the
        // abort has actual work to undo.
        let completer = FailsWhenRepliesRunOut {
            replies: Mutex::new(vec![tool_call_reply(
                "kb_write_page",
                serde_json::json!({
                    "path": "knowledge/sources/half-done.md",
                    "content": "---\ntitle: half\nkind: source\n---\n\nHalf.",
                    "commit_message": "half a digest"
                }),
            )]),
        };

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Note about HRV.".into(),
                    title: Some("HRV note".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("a provider error must fail the digest")
        .to_string();

        assert!(
            err.contains("could not decode"),
            "the provider's own explanation must reach the caller, got: {err}"
        );

        // HEAD is back on main and the transaction branch is gone — otherwise
        // every later write to this KB lands somewhere nobody is looking.
        let repo = git2::Repository::open(dir.path().join("k")).unwrap();
        assert_eq!(
            repo.head().unwrap().shorthand(),
            Some("main"),
            "a failed digest must leave HEAD on main"
        );
        let leftover: Vec<String> = repo
            .branches(Some(git2::BranchType::Local))
            .unwrap()
            .filter_map(|b| b.ok()?.0.name().ok()?.map(str::to_string))
            .filter(|n| n.starts_with("txn/"))
            .collect();
        assert!(
            leftover.is_empty(),
            "the transaction branch must be deleted; found: {leftover:?}"
        );

        // And the half-written page must not be visible in the knowledge base.
        let pages = crate::knowledge::store::list_pages(
            &paths::kb_root(svc.root(), "k"),
            Some("knowledge/"),
        )
        .unwrap();
        assert!(
            pages.is_empty(),
            "an aborted digest must leave no page behind; pages: {pages:?}"
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
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "Some note.".into(),
                    title: Some("y".into()),
                },
                completer: Box::new(completer),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
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

    // ── Issue #56, Task 10B: CP2 ────────────────────────────────────────────

    /// A completer that never answers. The sub-agent run fails immediately, so
    /// this test says nothing about the macro's happy path and everything about
    /// WHEN the raise runs.
    struct RefusesImmediately;

    #[async_trait]
    impl Completer for RefusesImmediately {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[crate::knowledge::subagent::loop_::LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            anyhow::bail!("no model here")
        }
    }

    #[tokio::test]
    async fn the_ingest_macro_ratchets_before_its_sub_agent_runs() {
        // CP2, and the reason it exists. The sub-agent writes through
        // KbToolDispatch → store::write_page / svc.add_raw_source, which no MCP
        // tool gate can see. This is also the test that makes Task 11's headline
        // test reachable: `conversation_ingest::ingest_conversation` funnels into
        // this function, as do the four HTTP macro routes, the CLI and the probe.
        let (dir, svc) = fresh_svc();
        let root = dir.path().to_path_buf();
        assert!(!crate::knowledge::tier::is_private(&root, "k"));

        let _ = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: true,
                source: SourceInput::Text {
                    text: "n=412".into(),
                    title: Some("t".into()),
                },
                completer: Box::new(RefusesImmediately),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await; // the sub-agent fails; the raise stands

        assert!(
            crate::knowledge::tier::is_private(&root, "k"),
            "the raise ran after the sub-agent, or not at all"
        );
    }

    #[tokio::test]
    async fn a_public_ingest_never_lowers_a_ratcheted_base() {
        let (dir, svc) = fresh_svc();
        let root = dir.path().to_path_buf();
        crate::knowledge::tier::raise_unlocked(&root, "k", true).unwrap();

        let _ = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                source: SourceInput::Text {
                    text: "public note".into(),
                    title: Some("t".into()),
                },
                completer: Box::new(RefusesImmediately),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await;

        assert!(
            crate::knowledge::tier::is_private(&root, "k"),
            "a public ingest lowered the tier"
        );
    }
}
