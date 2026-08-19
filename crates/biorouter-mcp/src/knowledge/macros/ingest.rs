//! `ingest` macro — wraps the sub-agent loop to integrate a new source into
//! a KB with a single, txn-atomic git commit.

use crate::knowledge::{
    biookf,
    convert::SourceInput,
    git::{GitRepo, Txn},
    manifest, paths,
    service::KnowledgeService,
    subagent::{
        events::{DoneReason, SubAgentEvent},
        kb_tools::{tool_specs, KbToolDispatch},
        loop_::{Completer, SubAgent, SubAgentBounds, SubAgentResult},
        procedures::{ingest_procedure, system_prompt},
    },
    types::{ChangeKind, KbFormat, SourceMeta},
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
    /// Whose agreements cover that model — DR-26's third axis (issue #56, Task
    /// 50). Required for the same reason `caller_is_private` is: an omission
    /// must be a compile error. `Unstated` is its `Default`, so a caller that
    /// cannot determine one fails closed.
    pub caller_affiliation: crate::knowledge::affiliation::CallerAffiliation,
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
    //
    // Task 10C (CP2): the barrier sits on the line ABOVE the ratchet, so a
    // public model never reaches the sub-agent's read tools at all — and never
    // reaches a raise that would stamp an entry-less directory explicitly
    // public. `conversation_ingest` builds these same `IngestArgs`, so it is
    // gated here too.
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

    // Materialize the raw source outside the sub-agent txn so it is durable
    // even if the sub-agent fails.
    let raw = svc
        .add_raw_source(&args.kb_id, args.source, None)
        .await
        .context("add_raw_source")?;

    // Open a transaction branch for the wiki-integration work.
    let repo = GitRepo::open(&kb_root)?;
    let txn = repo.begin_txn(&format!("ingest-{}", raw.source_id))?;

    // `Manifest::profile`, never `Manifest::format` — the field reads `Okf` on
    // every base written before Stage 3, so a legacy base would be taught OKF's
    // page contract and handed BioOKF's typed writer (DR-6's trap, reached from
    // the reader). `None` is legacy and gets the permissive path.
    let format = manifest::load(&kb_root).ok().and_then(|m| m.profile());

    // ⚠ Everything from here to the sub-agent runs INSIDE the transaction, so a
    // failure must abort it. `begin_txn` moves HEAD onto the txn branch, and an
    // early `?` would leave it parked there — which is how the next write to
    // this KB lands somewhere nobody is looking, the same hazard the failure
    // arms below already guard. The `schema.md` read predates this stage and had
    // exactly that bug; the two DR-24 steps join it rather than each growing
    // their own `match`.
    let setup = ingest_setup(svc, &kb_root, &repo, &txn, format, &raw.source_id);
    let (source_node, baseline_knowledge, schema) = match setup {
        Ok(setup) => setup,
        Err(e) => {
            let _ = repo.abort_txn(&txn);
            return Err(e);
        }
    };
    let system = system_prompt(&schema, ingest_procedure(format));

    let dispatch = KbToolDispatch {
        svc: svc.clone(),
        kb_id: args.kb_id.clone(),
        txn_branch: txn.branch.clone(),
    };
    let agent = SubAgent {
        completer: args.completer,
        tools: tool_specs(format),
        system_prompt: system,
        bounds: args.bounds,
    };
    let user = opening_message(&raw.source_id, args.focus.as_deref(), source_node.as_ref());

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
            let wrote_knowledge = match repo.txn_knowledge_tree_id(&txn) {
                Ok(after) => after != baseline_knowledge,
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

/// Everything the sub-agent needs that can fail, in one fallible call so the
/// transaction is aborted exactly once. See the call site for why that matters.
///
/// Returns the run's source node (BioOKF only), the `knowledge/` tree as the
/// sub-agent will find it, and the base's own `schema.md`.
fn ingest_setup(
    _svc: &KnowledgeService,
    kb_root: &std::path::Path,
    repo: &GitRepo,
    txn: &Txn,
    format: Option<KbFormat>,
    source_id: &str,
) -> Result<(Option<SourceNode>, Option<String>, String)> {
    // DR-24: in BioOKF mode the source node is a CONFORMANCE requirement, not a
    // step in a procedure the model may skip. Materialised inside the txn, so it
    // exists before the first edge that has to cite it — and so it goes with the
    // transaction if the run is aborted, rather than being left on main as an
    // orphan Publication nobody cites.
    let source_node = if format.is_some_and(KbFormat::is_biookf) {
        Some(materialize_source_node(kb_root, source_id, txn)?)
    } else {
        None
    };
    // …which means "did the sub-agent write anything" can no longer be asked
    // against main: the seed above already moved the `knowledge/` subtree, and a
    // check against main would answer yes for a run that did nothing (issue
    // #71). The baseline is the tree as the sub-agent finds it.
    let baseline_knowledge = repo.txn_knowledge_tree_id(txn)?;
    let schema = std::fs::read_to_string(kb_root.join("schema.md")).context("read schema.md")?;
    Ok((source_node, baseline_knowledge, schema))
}

// ---------------------------------------------------------------------------
// DR-24 — the source node
// ---------------------------------------------------------------------------

/// The sub-agent's first message: which source to integrate, any focus hints,
/// and — in BioOKF mode — the source node's identifier **spelled exactly**.
///
/// Exactly, because every edge the sub-agent writes has to reproduce that string
/// character for character in `primary_source` and in its `reported_in` object.
/// Telling it "cite the source node" and leaving it to reconstruct the name from
/// the source-id is how an ingest ends up with a base full of `primary_source`
/// values that resolve to nothing — one `biookf.edge.primary_source_unresolved`
/// per edge, from a run that looked like it went perfectly.
fn opening_message(
    source_id: &str,
    focus: Option<&str>,
    source_node: Option<&SourceNode>,
) -> String {
    let focus_line = focus.unwrap_or("");
    let mut user =
        format!("New source to integrate: source-id={source_id}. Focus hints: {focus_line}");
    if let Some(node) = source_node {
        user.push_str(&format!(
            "\n\nThe source node for it already exists at {} with identifier: {}\n\
             Use that identifier VERBATIM as `primary_source` on every edge, and give every \
             page you write a `reported_in` edge whose `object` is that same string. Extend \
             that page (type, xref, description) if you learn more about the source itself; do \
             not create a second node for it.",
            node.path, node.identifier
        ));
    }
    user
}

/// Where a run's source node landed, and what every edge must cite.
pub struct SourceNode {
    /// The `identifier` other pages join to. Not the source-id, and not the
    /// path: BioOKF §7.2 joins on `identifier` and nothing else.
    pub identifier: String,
    /// Bundle-relative path, so the sub-agent can read and extend it.
    pub path: String,
}

/// Materialise the ingested source as a real, typed **concept page** (DR-24).
///
/// ## Why this is Rust and not a step in the procedure
///
/// It is a conformance requirement with a hard consequence. BioOKF §8 requires
/// `primary_source` on every edge, and §8.1 requires it to name a
/// Publication/Study/Dataset/Agent **node that exists in the bundle**. If the
/// node is missing, every edge in the ingest fails conformance rule 4 and every
/// one of them raises `biookf.edge.primary_source_unresolved` — a whole ingest's
/// worth of findings from one omission, and an omission a model makes silently
/// because the edges it wrote look perfectly well-formed on their own.
///
/// Asking the procedure to create it first makes the base's conformance
/// contingent on the model having read step 2, on a loop that is bounded at 30
/// steps and may lose the instruction to a long source. Writing it here makes it
/// a floor: the model may improve the page — refine `Publication` to `Study`,
/// add an `xref` it found in the text — and cannot fail to have one.
///
/// ## Provenance, both mechanisms
///
/// The page carries `raw_source`, which anchors the chain to the immutable bytes
/// under `raw/`, and its own `reported_in` edge citing **itself** as
/// `primary_source`. That self-reference is SPEC §8.1's intended terminating
/// base case, not a cycle: a source attests its own contents, and the lint knows
/// it (`check_primary_source` resolves it to a source-typed node and says
/// nothing). Without the self-edge the source node would be the one page in the
/// bundle with no `reported_in`, which reads as an omission rather than as a
/// terminus.
fn materialize_source_node(
    kb_root: &std::path::Path,
    source_id: &str,
    txn: &Txn,
) -> Result<SourceNode> {
    let meta = crate::knowledge::raw::read_meta(kb_root, source_id)
        .with_context(|| format!("read raw/{source_id}/meta.yaml for the source node"))?;
    let identifier = source_identifier(&meta);
    let node_type = source_node_type(&meta);
    let path = format!(
        "knowledge/{}/{}.md",
        node_type.as_str().to_lowercase(),
        source_id
    );
    let content = source_page(&identifier, node_type, &meta, source_id);
    crate::knowledge::store::write_page(
        kb_root,
        &path,
        &content,
        &format!("source node for {source_id}"),
        Some(&txn.branch),
    )?;
    Ok(SourceNode { identifier, path })
}

/// The node's `identifier`: the source's own title when it has one.
///
/// §7.1 wants it human-readable, which the source-id is not — `chen-2020-il6-a1b2c3`
/// is exactly the opaque code that rule exists to keep out of `identifier` and in
/// `xref`. The id is the fallback only because a titleless source has to be
/// called something, and being unresolvable is worse than being ugly.
fn source_identifier(meta: &SourceMeta) -> String {
    let title = meta.title.trim();
    if title.is_empty() {
        meta.id.clone()
    } else {
        title.to_string()
    }
}

/// Which of §8.1's four source types this is.
///
/// Deliberately coarse. Only four types may bear a `primary_source`, and of them
/// only `Dataset` is decidable from a mime type — a CSV or a spreadsheet is
/// data, everything else is something that was *written*. `Publication` is the
/// honest default for a document of unknown kind, and the procedure tells the
/// sub-agent it may refine it (a trial write-up is a `Study`, an authority like
/// HGNC is an `Agent`) once it has read the text. Guessing `Study` from a title
/// containing "trial" would be a heuristic that is wrong quietly.
fn source_node_type(meta: &SourceMeta) -> biookf::NodeType {
    let mime = meta.mime.to_ascii_lowercase();
    let data = mime.contains("csv")
        || mime.contains("tab-separated")
        || mime.contains("spreadsheet")
        || mime.contains("json")
        || mime.contains("parquet");
    if data {
        biookf::NodeType::Dataset
    } else {
        biookf::NodeType::Publication
    }
}

/// The page itself, composed through `serde_yaml` for the same reason
/// `kb_write_concept` does: a title carrying `: ` written into frontmatter by
/// hand produces an unparseable block, and an unparseable block does not fail —
/// the page simply stops being in the graph.
fn source_page(
    identifier: &str,
    node_type: biookf::NodeType,
    meta: &SourceMeta,
    source_id: &str,
) -> String {
    let mut fm = serde_yaml::Mapping::new();
    fm.insert("type".into(), node_type.as_str().into());
    fm.insert("identifier".into(), identifier.into());
    fm.insert(
        "description".into(),
        format!("Source ingested into this knowledge base as `{source_id}`.").into(),
    );
    let xrefs = source_xrefs(meta);
    if !xrefs.is_empty() {
        fm.insert(
            "xref".into(),
            serde_yaml::Value::Sequence(xrefs.iter().map(|x| x.as_str().into()).collect()),
        );
    }
    fm.insert(
        "raw_source".into(),
        serde_yaml::Value::Sequence(vec![format!("raw/{source_id}/source.md").into()]),
    );

    // §8.1's terminating self-citation; see this module's `materialize_source_node`.
    let mut edge = serde_yaml::Mapping::new();
    edge.insert("predicate".into(), "reported_in".into());
    edge.insert("object".into(), identifier.into());
    edge.insert("knowledge_level".into(), "knowledge_assertion".into());
    edge.insert("agent_type".into(), "automated_agent".into());
    edge.insert("primary_source".into(), identifier.into());
    fm.insert(
        "edges".into(),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(edge)]),
    );

    let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(fm))
        .unwrap_or_else(|_| format!("type: {}\n", node_type.as_str()));
    let mut body = format!("---\n{yaml}---\n\n# {identifier}\n\n");
    body.push_str(&format!(
        "Ingested source `{source_id}`. The original is at `raw/{source_id}/`.\n"
    ));
    if let Some(url) = meta.url.as_deref().filter(|u| !u.is_empty()) {
        body.push_str(&format!("\nRetrieved from <{url}>.\n"));
    }
    body
}

/// PMID / DOI / arXiv / ISBN, read out of the **title and URL** with the same
/// extractor the credibility classifier already uses.
///
/// One extractor and not a second regex here: two would answer differently
/// about the same source the first time either was tweaked, and an `xref` that
/// disagrees with the credibility record about which paper this is, is worse
/// than an absent one. `xref` is where §7.1 wants the codes — and the reason
/// `identifier` may stay a human-readable title.
///
/// ⚠ **The document body is deliberately not scanned**, and the omission costs
/// real hits: a PDF states its own DOI in the text far more often than in its
/// filename. It is still the right call, because a paper's *reference list* is
/// full of other papers' identifiers, and a regex cannot tell a work's own DOI
/// from one it cites. A missing `xref` is an enrichment opportunity — the node
/// is already anchored by `raw_source`, so `biookf.source.unanchored` does not
/// fire — while a **wrong** one is a false claim about which paper this is,
/// propagated to every edge that cites the node. Reading the document and
/// judging which identifier is its own is a job for the sub-agent, which is why
/// the procedure tells it to extend this page when it learns more.
fn source_xrefs(meta: &SourceMeta) -> Vec<String> {
    let haystack = format!("{} {}", meta.title, meta.url.as_deref().unwrap_or_default());
    let ids = crate::knowledge::credibility::identifiers::extract(&haystack);
    let mut out = Vec::new();
    if let Some(pmid) = ids.pmid {
        out.push(format!("PMID:{pmid}"));
    }
    if let Some(doi) = ids.doi {
        out.push(format!("doi:{doi}"));
    }
    if let Some(arxiv) = ids.arxiv {
        out.push(format!("arXiv:{arxiv}"));
    }
    if let Some(isbn) = ids.isbn {
        out.push(format!("ISBN:{isbn}"));
    }
    out
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
        page_fixtures::valid_page,
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
                    "content": valid_page("source", "stub", "Stub."),
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
                caller_affiliation: Default::default(),
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
    // DR-24: the source node is materialised, not asked for
    // -------------------------------------------------------------------------

    fn biookf_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_as(
            "bio",
            "Bio",
            None,
            KbFormat::Biookf,
            false,
            &Default::default(),
        )
        .unwrap();
        (dir, svc)
    }

    fn biookf_args(completer: MockCompleter) -> IngestArgs {
        IngestArgs {
            kb_id: "bio".into(),
            caller_is_private: false,
            caller_affiliation: Default::default(),
            source: SourceInput::Text {
                text: "Tocilizumab reduced mortality in severe COVID-19. PMID: 33933206".into(),
                title: Some("RECOVERY: tocilizumab in COVID-19".into()),
            },
            completer: Box::new(completer),
            focus: None,
            bounds: SubAgentBounds::default(),
            event_sink: None,
            cancel: None,
        }
    }

    /// The conformance floor. Every edge a BioOKF ingest writes must cite a
    /// `primary_source` that resolves to a Publication/Study/Dataset/Agent page
    /// **in the bundle** (§8.1); without one, every edge in the run raises
    /// `biookf.edge.primary_source_unresolved` — a whole ingest's worth of
    /// findings from one omission the model makes silently, because the edges it
    /// wrote look well-formed on their own.
    #[tokio::test]
    async fn a_biookf_ingest_materializes_the_source_node_before_the_sub_agent_runs() {
        let (_dir, svc) = biookf_svc();
        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_concept",
                serde_json::json!({
                    "type": "Molecule",
                    "identifier": "Tocilizumab",
                    "edges": [{
                        "predicate": "reported_in",
                        "object": "RECOVERY: tocilizumab in COVID-19",
                        "knowledge_level": "knowledge_assertion",
                        "agent_type": "text_mining_agent",
                        "primary_source": "RECOVERY: tocilizumab in COVID-19",
                    }],
                }),
            ),
            text_reply("done"),
        ]);
        ingest(&svc, biookf_args(completer)).await.unwrap();

        let kb = svc.root().join("bio");
        let pages = crate::knowledge::validate::load_bundle(&kb).unwrap();
        let source = pages
            .iter()
            .find(|p| p.path.starts_with("knowledge/publication/"))
            .expect("the source node was materialised");

        // Its identifier is the source's own title, not the opaque source-id
        // §7.1 keeps out of `identifier` and in `xref`.
        assert_eq!(
            source.doc.identifier.as_deref(),
            Some("RECOVERY: tocilizumab in COVID-19")
        );
        // `raw_source` anchors the chain to the immutable bytes — which is
        // what makes an absent `xref` an enrichment opportunity rather than an
        // unanchored source. See `source_xrefs` for why the body is not
        // scanned for identifiers.
        assert!(source.doc.xref.is_empty(), "{:?}", source.doc.xref);
        let text = std::fs::read_to_string(kb.join(&source.path)).unwrap();
        assert!(text.contains("raw_source:"), "{text}");
        assert!(text.contains("/source.md"), "{text}");

        // DR-24 names TWO provenance mechanisms and DR-4 carried only one, so
        // the source node's own `reported_in` edge is asserted rather than
        // assumed: without it the source is the one page in the bundle with no
        // `reported_in`, which reads as an omission rather than as the chain's
        // terminus. It cites ITSELF as `primary_source` — §8.1's intended
        // terminating base case.
        let self_edge = source
            .doc
            .edges
            .iter()
            .find(|e| e.predicate == "reported_in")
            .expect("the source node carries its own reported_in edge");
        assert_eq!(self_edge.object, "RECOVERY: tocilizumab in COVID-19");
        assert_eq!(
            self_edge.primary_source.as_deref(),
            Some("RECOVERY: tocilizumab in COVID-19"),
            "a source attests its own contents"
        );

        // And the whole bundle is conformant: no edge cites a source that does
        // not resolve, and that self-citation is not reported as a dangling one.
        let diagnostics = crate::knowledge::validate::validate_page(
            Some(KbFormat::Biookf),
            Some(&source.path),
            &text,
            &pages,
        );
        assert_eq!(
            diagnostics.errors(),
            0,
            "the source node must be conformant: {:#?}",
            diagnostics.items
        );
        assert!(
            !diagnostics.has("biookf.edge.primary_source_unresolved"),
            "the self-citation is the intended terminus, not a dangling source: {:#?}",
            diagnostics.items
        );
    }

    /// The identifiers that *are* safe to read: the ones in the source's own
    /// title and URL, which name this document and not one it cites.
    #[test]
    fn a_source_url_or_title_carrying_an_identifier_becomes_an_xref() {
        let meta = |title: &str, url: Option<&str>| SourceMeta {
            id: "s".into(),
            title: title.into(),
            url: url.map(str::to_string),
            ingested_at: chrono::Utc::now(),
            sha256: String::new(),
            mime: "text/markdown".into(),
            original_filename: None,
            credibility: crate::knowledge::types::Credibility {
                tier: crate::knowledge::types::CredibilityTier::Web,
                confidence: 0.0,
                publisher: None,
                venue: None,
                doi: None,
                retracted: false,
                reasoning: String::new(),
                classifier_version: 1,
            },
        };
        assert_eq!(
            source_xrefs(&meta(
                "Tocilizumab in COVID-19",
                Some("https://pubmed.ncbi.nlm.nih.gov/33933206"),
            )),
            vec!["PMID:33933206".to_string()]
        );
        assert_eq!(
            source_xrefs(&meta("Something (10.1016/S0140-6736(21)00676-0)", None)),
            vec!["doi:10.1016/s0140-6736(21)00676-0".to_string()]
        );
        assert!(source_xrefs(&meta("An untitled note", None)).is_empty());
    }

    /// ⚠ **The regression the seed creates.** `txn_wrote_knowledge_pages`
    /// compares the txn branch's `knowledge/` subtree against **main**, which is
    /// the right question only while main is the last thing that wrote
    /// knowledge. Seeding the source node moves that subtree before the
    /// sub-agent starts — so a run in which the model wrote nothing at all would
    /// pass the check, commit, and hand the caller a sha for work that never
    /// happened. That is exactly the false success issue #71 closed.
    #[tokio::test]
    async fn the_seeded_source_node_does_not_by_itself_count_as_a_digest() {
        let (_dir, svc) = biookf_svc();
        // The model reads and then gives up: no page written.
        let completer = MockCompleter::new(vec![
            tool_call_reply("kb_list_pages", serde_json::json!({})),
            text_reply("I could not find anything to record."),
        ]);
        let err = ingest(&svc, biookf_args(completer))
            .await
            .expect_err("a run that wrote no knowledge must fail, seed or no seed");
        assert!(
            err.to_string().contains("wrote no knowledge pages"),
            "{err}"
        );

        // …and the seed went with the aborted transaction rather than being
        // left on main as an orphan Publication nobody cites.
        let kb = svc.root().join("bio");
        let orphans =
            crate::knowledge::store::list_pages(&kb, Some("knowledge/publication")).unwrap();
        assert!(
            orphans.is_empty(),
            "the aborted txn left the source node behind: {orphans:?}"
        );
    }

    /// ⚠ **A failure between `begin_txn` and the sub-agent leaves HEAD parked on
    /// the transaction branch**, and every later write to this KB then lands
    /// somewhere nobody is looking. The `schema.md` read has been on that stretch
    /// since the macro was written and never aborted; Stage 5 put two more
    /// fallible steps beside it, so all three now go through `ingest_setup` and
    /// the transaction is aborted exactly once.
    ///
    /// Provoked by making `schema.md` a directory, which is the cheapest way to
    /// make exactly that read fail while everything before it succeeds.
    #[tokio::test]
    async fn a_failure_setting_up_the_transaction_still_leaves_head_on_main() {
        let (dir, svc) = biookf_svc();
        let schema = dir.path().join("bio/schema.md");
        std::fs::remove_file(&schema).unwrap();
        std::fs::create_dir(&schema).unwrap();

        let completer = MockCompleter::new(vec![text_reply("never reached")]);
        let err = ingest(&svc, biookf_args(completer))
            .await
            .expect_err("an unreadable schema.md fails the digest");
        assert!(err.to_string().contains("schema.md"), "{err}");

        let repo = git2::Repository::open(dir.path().join("bio")).unwrap();
        assert_eq!(
            repo.head().unwrap().shorthand(),
            Some("main"),
            "a failed setup must leave HEAD on main"
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
    }

    /// An OKF base has no closed vocabulary and no §8.1 source-node rule, so it
    /// gets neither the seed nor the typed writer. The negative half: a source
    /// node minted into an OKF base would be a `Publication`-typed page in a
    /// base whose types are the user's own.
    #[tokio::test]
    async fn an_okf_ingest_seeds_nothing_and_keeps_the_permissive_tools() {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base_as(
            "okf",
            "Okf",
            None,
            KbFormat::Okf,
            false,
            &Default::default(),
        )
        .unwrap();
        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_page",
                serde_json::json!({
                    "path": "knowledge/source/note.md",
                    "content": valid_page("Source", "Note", "Body."),
                }),
            ),
            text_reply("done"),
        ]);
        ingest(
            &svc,
            IngestArgs {
                kb_id: "okf".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
                source: SourceInput::Text {
                    text: "A note.".into(),
                    title: Some("A note".into()),
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
        let kb = svc.root().join("okf");
        assert!(
            crate::knowledge::store::list_pages(&kb, Some("knowledge/publication"))
                .unwrap()
                .is_empty(),
            "an OKF base must not be given a BioOKF source node"
        );
    }

    /// A CSV is data. The only one of §8.1's four source types that is decidable
    /// from a mime type — everything else was *written*, and guessing `Study`
    /// from a title containing "trial" would be a heuristic that is wrong
    /// quietly.
    #[tokio::test]
    async fn a_tabular_source_is_typed_dataset_and_a_document_is_typed_publication() {
        let (_dir, svc) = biookf_svc();
        let completer = MockCompleter::new(vec![
            tool_call_reply(
                "kb_write_concept",
                serde_json::json!({ "type": "Gene", "identifier": "IL6" }),
            ),
            text_reply("done"),
        ]);
        let mut args = biookf_args(completer);
        args.source = SourceInput::File {
            bytes: b"gene,effect\nIL6,0.3\n".to_vec(),
            filename: "effects.csv".into(),
            mime: Some("text/csv".into()),
        };
        ingest(&svc, args).await.unwrap();
        let kb = svc.root().join("bio");
        assert!(
            !crate::knowledge::store::list_pages(&kb, Some("knowledge/dataset"))
                .unwrap()
                .is_empty(),
            "a csv source is a Dataset node"
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
                caller_affiliation: Default::default(),
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
                caller_affiliation: Default::default(),
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
                caller_affiliation: Default::default(),
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
                    // A conformant body on purpose. The one thing wrong with
                    // this call is its path, and a body the writer would also
                    // refuse (Stage 3's validator, DR-19) would satisfy the
                    // assertion below with the traversal guard removed.
                    "path": "knowledge/../escape.md",
                    "content": valid_page("source", "Escape", "Body."),
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
                caller_affiliation: Default::default(),
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
                caller_affiliation: Default::default(),
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
                caller_affiliation: Default::default(),
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
                    "content": valid_page("source", "half", "Half."),
                    "commit_message": "half a digest"
                }),
            )]),
        };

        let err = ingest(
            &svc,
            IngestArgs {
                kb_id: "k".into(),
                caller_is_private: false,
                caller_affiliation: Default::default(),
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
                    "content": valid_page("source", "Escape", "Body."),
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
                caller_affiliation: Default::default(),
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
                caller_affiliation: Default::default(),
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
                caller_affiliation: Default::default(),
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
