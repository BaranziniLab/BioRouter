//! Digest Biorouter conversation/chat history into a knowledge base.
//!
//! The knowledge `ingest` macro already knows how to turn an arbitrary text
//! source into wiki pages. This module renders a stored [`Session`]'s full
//! conversation — user turns, model turns, tool calls (with their arguments)
//! and tool responses — into a single markdown document and feeds it through
//! that same pipeline. Because it produces a plain `SourceInput::Text`, every
//! downstream behaviour (credibility, dedup, sub-agent digestion, git history)
//! is identical to ingesting a pasted note.
//!
//! It is the shared backend for three surfaces:
//!   * the HTTP route `POST /knowledge/bases/{id}/ingest-conversation` (GUI),
//!   * the CLI `biorouter knowledge ingest-conversation`,
//!   * the `kb_ingest_conversation` chat tool ("just say the word").

use crate::conversation::message::{ActionRequiredData, MessageContent};
use crate::session::session_manager::Session;
use biorouter_mcp::knowledge::{
    convert::SourceInput,
    macros::ingest::{ingest, IngestArgs, IngestResult},
    service::KnowledgeService,
    subagent::{events::SubAgentEvent, loop_::Completer, loop_::SubAgentBounds},
};
use rmcp::model::Role;

/// A conversation rendered to a digestible markdown document.
#[derive(Debug, Clone)]
pub struct RenderedConversation {
    pub title: String,
    pub markdown: String,
    /// Number of messages that contributed content (empty turns are skipped).
    pub rendered_messages: usize,
}

/// Render a single session's conversation into markdown.
///
/// Captures, in transcript order: user input, model output, every tool call
/// (name + arguments) and every tool response. Thinking blocks and pure-UI
/// notifications are omitted — they carry no durable knowledge.
pub fn render_conversation(session: &Session) -> RenderedConversation {
    let title = conversation_title(session);
    let mut out = String::new();
    out.push_str(&format!("# Conversation: {title}\n\n"));
    out.push_str(&format!(
        "_Session `{}` · started {} · working dir `{}`_\n\n",
        session.id,
        session.created_at.format("%Y-%m-%d %H:%M UTC"),
        session.working_dir.display()
    ));

    let mut rendered = 0usize;
    if let Some(convo) = &session.conversation {
        for msg in convo.messages() {
            let speaker = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
            };
            let mut blocks: Vec<String> = Vec::new();
            for content in &msg.content {
                match content {
                    MessageContent::Text(t) => {
                        let t = t.text.trim();
                        if !t.is_empty() {
                            blocks.push(t.to_string());
                        }
                    }
                    MessageContent::ToolRequest(req) => {
                        if let Ok(call) = &req.tool_call {
                            let args = serde_json::to_string_pretty(&call.arguments)
                                .unwrap_or_else(|_| "{}".into());
                            blocks.push(format!(
                                "**Tool call → `{}`**\n\n```json\n{}\n```",
                                call.name,
                                args.trim()
                            ));
                        }
                    }
                    MessageContent::FrontendToolRequest(req) => {
                        if let Ok(call) = &req.tool_call {
                            blocks.push(format!("**Tool call → `{}`** (frontend)", call.name));
                        }
                    }
                    MessageContent::ToolResponse(resp) => {
                        let body = match &resp.tool_result {
                            Ok(result) => {
                                let text = result
                                    .content
                                    .iter()
                                    .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                if text.trim().is_empty() {
                                    format!("({} non-text content item(s))", result.content.len())
                                } else {
                                    text
                                }
                            }
                            Err(e) => format!("Error: {e}"),
                        };
                        blocks.push(format!(
                            "**Tool response**\n\n```\n{}\n```",
                            truncate_block(body.trim(), 6000)
                        ));
                    }
                    MessageContent::ActionRequired(a) => {
                        if let ActionRequiredData::Elicitation { message, .. } = &a.data {
                            blocks.push(format!("**Action required:** {message}"));
                        }
                    }
                    // Thinking / redacted thinking / images / system notifications
                    // carry no durable knowledge — skip.
                    _ => {}
                }
            }
            if blocks.is_empty() {
                continue;
            }
            rendered += 1;
            out.push_str(&format!("## {speaker}\n\n"));
            out.push_str(&blocks.join("\n\n"));
            out.push_str("\n\n");
        }
    }

    RenderedConversation {
        title,
        markdown: out,
        rendered_messages: rendered,
    }
}

/// Render one or more sessions into a single markdown document. Used when the
/// caller wants every selected conversation folded into one source.
pub fn render_conversations(sessions: &[Session]) -> RenderedConversation {
    if sessions.len() == 1 {
        return render_conversation(&sessions[0]);
    }
    let mut markdown = String::new();
    let mut total = 0usize;
    for s in sessions {
        let r = render_conversation(s);
        total += r.rendered_messages;
        markdown.push_str(&r.markdown);
        markdown.push_str("\n---\n\n");
    }
    let title = format!("{} conversations", sessions.len());
    RenderedConversation {
        title,
        markdown,
        rendered_messages: total,
    }
}

fn conversation_title(session: &Session) -> String {
    let name = session.name.trim();
    if !name.is_empty() && name.to_lowercase() != "untitled" {
        return name.to_string();
    }
    format!("Chat {}", session.created_at.format("%Y-%m-%d %H:%M"))
}

fn truncate_block(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}\n…[truncated]")
}

/// Arguments for [`ingest_conversation`], mirroring the ingest-macro options
/// the caller already controls.
pub struct ConversationIngestArgs {
    pub kb_id: String,
    /// The capability of whoever is asking (issue #56). Added by Task 10B
    /// because `ingest_conversation` must have something to put in
    /// `IngestArgs.caller_is_private`; **Task 11 adds the refusal that consumes
    /// it**, and the two are deliberately separate — this task plumbs, Task 11
    /// gates, exactly as 10B/10C split for the KB choke points.
    ///
    /// Required and non-`Option`, so all three production constructors (the
    /// platform tool, `POST /ingest-conversation`, the CLI) are a compile error
    /// rather than an omission. A hardcoded `false` here would reproduce
    /// verbatim the failure this task exists to prevent: every file would
    /// report a non-zero `caller_is_private` count while ratcheting nothing.
    pub caller_capability: crate::privacy::ProviderTier,
    pub sessions: Vec<Session>,
    pub completer: Box<dyn Completer>,
    pub focus: Option<String>,
    pub bounds: SubAgentBounds,
    pub event_sink: Option<tokio::sync::mpsc::UnboundedSender<SubAgentEvent>>,
    pub cancel: Option<std::sync::Arc<tokio::sync::Notify>>,
}

/// What an ingest of other sessions' conversations produced, plus how many were
/// refused by Gate G.
///
/// ⚠ NOT a `refused` field on [`IngestResult`]. That type lives in
/// `biorouter-mcp` (`knowledge/macros/ingest.rs`), derives
/// `Serialize`/`Deserialize`, and is the payload of the SSE macro routes — a
/// field there is a wire change to three routes that have nothing to do with
/// Gate G, in a crate this change touches no file of. All three callers of
/// [`ingest_conversation`] are already being edited here, so changing its
/// RETURN type is the cheaper edit and the honest one.
///
/// `#[serde(flatten)]` keeps the `/knowledge/bases/{id}/ingest-conversation`
/// SSE result payload byte-identical to what the GUI already parses, and simply
/// adds `refused` beside it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationIngestResult {
    #[serde(flatten)]
    pub ingested: IngestResult,
    /// How many of the requested sessions the barrier refused. A count and
    /// nothing else — §11.4 classifies a session's id, title and working
    /// directory as content, and this product's titles are LLM-generated from
    /// the conversation itself.
    #[serde(default)]
    pub refused: usize,
}

/// Names no session, no title and no working directory — §11.4 classifies all
/// three as content, and a session title in this product is LLM-generated from
/// the conversation itself.
pub const REFUSED_ALL_PRIVATE: &str = "\
Those chats are private: they were created under a model hosted inside the institution, so only a \
private model may read them. This session is running on a public model. Ask the user to switch this \
chat to a private model and try again.";

/// Gate G's predicate, and the **one** place `visible_to` is consulted for a
/// conversation ingest (issue #56).
fn readable(caller: crate::privacy::ProviderTier, session: &Session) -> bool {
    crate::privacy::visible_to(caller, session.privacy_tier)
}

/// Would Gate G refuse **every** one of these sessions?
///
/// The barrier itself is inside [`ingest_conversation`] — that is what covers
/// the CLI and every non-HTTP caller, and it is the check a `grep` counts. This
/// is the same question asked one layer up so the HTTP route can answer with a
/// real status code instead of an SSE stream that opens and immediately dies;
/// exactly the role `assert_macro_target_reachable` plays for Task 10C's
/// barrier. It re-uses [`readable`], so there is no second spelling of the rule.
pub fn refuses_every_session(caller: crate::privacy::ProviderTier, sessions: &[Session]) -> bool {
    !sessions.is_empty() && !sessions.iter().any(|s| readable(caller, s))
}

/// Render the selected session(s) and ingest them into `kb_id` as one source,
/// reusing the standard knowledge ingest macro.
pub async fn ingest_conversation(
    svc: &KnowledgeService,
    args: ConversationIngestArgs,
) -> anyhow::Result<ConversationIngestResult> {
    if args.sessions.is_empty() {
        anyhow::bail!("no conversations selected for ingestion");
    }

    // Issue #56 Gate G. Per session, not once: `sessions` is a caller-supplied
    // LIST, and a single up-front check on the first element admits the rest.
    // Placed before `render_conversations` so no refused transcript is ever
    // rendered into a buffer, even one that is then dropped.
    let caller = args.caller_capability;
    let (allowed, refused): (Vec<Session>, Vec<Session>) =
        args.sessions.into_iter().partition(|s| readable(caller, s));
    if allowed.is_empty() && !refused.is_empty() {
        anyhow::bail!("{}", REFUSED_ALL_PRIVATE);
    }
    let refused = refused.len();

    let rendered = render_conversations(&allowed);
    if rendered.rendered_messages == 0 {
        anyhow::bail!("selected conversation(s) contain no digestible content");
    }
    let focus = args.focus.or_else(|| {
        Some(
            "This source is a Biorouter chat transcript. Capture the user's questions, \
             the approach taken, the tools and commands used, their results, and any \
             durable findings or preferences revealed."
                .to_string(),
        )
    });
    let ingested = ingest(
        svc,
        IngestArgs {
            kb_id: args.kb_id,
            // Issue #56. The ProviderTier -> bool crossing, and the only one:
            // `IngestArgs` lives in biorouter-mcp, which cannot name
            // ProviderTier. This same value drove the refusal above.
            caller_is_private: caller.is_private(),
            source: SourceInput::Text {
                text: rendered.markdown,
                title: Some(format!("Conversation — {}", rendered.title)),
            },
            completer: args.completer,
            focus,
            bounds: args.bounds,
            event_sink: args.event_sink,
            cancel: args.cancel,
        },
    )
    .await?;
    Ok(ConversationIngestResult { ingested, refused })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use crate::conversation::Conversation;
    use crate::session::session_manager::{Session, SessionType};
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content};
    use std::path::PathBuf;

    fn base_session() -> Session {
        Session {
            id: "sess-1".into(),
            working_dir: PathBuf::from("/tmp/x"),
            name: "Analysing HRV".into(),
            user_set_name: true,
            session_type: SessionType::User,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            extension_data: Default::default(),
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            schedule_id: None,
            workflow: None,
            user_workflow_values: None,
            conversation: None,
            message_count: 0,
            provider_name: None,
            model_config: None,
            diverged_from: None,
            branch_point_msg_uid: None,
            parent_session_id: None,
            privacy_tier: crate::privacy::SessionClassification::Public,
            privacy_reason: None,
        }
    }

    #[test]
    fn renders_user_assistant_and_tool_turns() {
        let mut s = base_session();
        let user = Message::user().with_text("What is my resting HRV trend?");
        let tool_call = Message::assistant().with_content(MessageContent::ToolRequest(
            crate::conversation::message::ToolRequest {
                id: "t1".into(),
                tool_call: Ok(CallToolRequestParams {
                    task: None,
                    name: "shell".into(),
                    arguments: Some(
                        serde_json::json!({ "command": "cat hrv.csv" })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                    meta: None,
                }),
                metadata: None,
                tool_meta: None,
            },
        ));
        let tool_resp = Message::user().with_content(MessageContent::ToolResponse(
            crate::conversation::message::ToolResponse {
                id: "t1".into(),
                tool_result: Ok(CallToolResult::success(vec![Content::text(
                    "day,hrv\n1,55\n2,60",
                )])),
                metadata: None,
            },
        ));
        let answer = Message::assistant().with_text("Your HRV is trending upward.");
        s.conversation = Some(Conversation::new_unvalidated(vec![
            user, tool_call, tool_resp, answer,
        ]));

        let r = render_conversation(&s);
        assert!(r.markdown.contains("# Conversation: Analysing HRV"));
        assert!(r.markdown.contains("## User"));
        assert!(r.markdown.contains("resting HRV trend"));
        assert!(r.markdown.contains("Tool call → `shell`"));
        assert!(r.markdown.contains("cat hrv.csv"));
        assert!(r.markdown.contains("Tool response"));
        assert!(r.markdown.contains("day,hrv"));
        assert!(r.markdown.contains("trending upward"));
        assert!(r.rendered_messages >= 3);
    }

    #[test]
    fn skips_empty_and_thinking_only_turns() {
        let mut s = base_session();
        let thinking = Message::assistant().with_thinking("internal reasoning", "sig");
        s.conversation = Some(Conversation::new_unvalidated(vec![thinking]));
        let r = render_conversation(&s);
        assert_eq!(r.rendered_messages, 0);
    }

    /// A completer whose single reply carries text and no tool calls — what a
    /// model hands back when its request failed, or when it simply decides the
    /// transcript is not worth writing up.
    struct SilentCompleter;

    #[async_trait::async_trait]
    impl Completer for SilentCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[biorouter_mcp::knowledge::subagent::loop_::LlmMessage],
            _tools: &[rmcp::model::Tool],
        ) -> anyhow::Result<biorouter_mcp::knowledge::subagent::loop_::LlmReply> {
            Ok(biorouter_mcp::knowledge::subagent::loop_::LlmReply {
                text: "The provider request failed.".into(),
                tool_calls: Vec::new(),
            })
        }
    }

    /// Issue #70. The Meditation workflow's whole write path is
    /// `platform__ingest_conversation` → this function → the `ingest` macro, and
    /// the macro used to squash-commit an unchanged tree and hand back a commit
    /// sha. `ingest_summary` then told the agent "Ingested 1 conversation(s) into
    /// knowledge base 'soul'. … commit: <sha>", the workflow reported success,
    /// and the Soul knowledge base had gained nothing but a raw transcript. It is
    /// the same defect as #71, reached through a different door, so it is pinned
    /// from this side too.
    #[tokio::test]
    async fn a_conversation_digest_that_wrote_nothing_is_not_reported_as_ingested() {
        let tmp = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("soul", "Soul", None).unwrap();

        let mut session = base_session();
        session.conversation = Some(Conversation::new_unvalidated(vec![
            Message::user().with_text("I always reach for ggplot2 before base R."),
            Message::assistant().with_text("Noted."),
        ]));

        let err = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: "soul".into(),
                caller_capability: crate::privacy::ProviderTier::Public,
                sessions: vec![session],
                completer: Box::new(SilentCompleter),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("a Meditation that wrote no Soul page must not report an ingest")
        .to_string();

        assert!(
            err.contains("no knowledge pages"),
            "the failure must say the digest wrote nothing, got: {err}"
        );

        // And the claim must hold: the Soul has no knowledge page.
        let sources = tmp.path().join("soul/knowledge/sources");
        assert!(
            std::fs::read_dir(&sources)
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
            "no Soul page may exist after a failed Meditation"
        );
    }

    // ── Issue #56, Gate G: cross-session conversation ingest ────────────────
    //
    // `platform__ingest_conversation` is dispatched inside `agent.rs` BEFORE the
    // extension-manager fall-through, so it is not an MCP tool and `filter_tools`
    // cannot hide it; it never touches `chat_history_search.rs` either. Gates C,
    // D and E all miss it by construction. Its sink is worse than its source: a
    // knowledge base is a machine-wide tree any session may name.

    use crate::privacy::{ProviderTier, SessionClassification};

    fn session_with(
        id: &str,
        tier: SessionClassification,
        text: &str,
    ) -> crate::session::session_manager::Session {
        let mut s = base_session();
        s.id = id.to_string();
        s.name = format!("chat {id}");
        s.privacy_tier = tier;
        s.conversation = Some(Conversation::new_unvalidated(vec![
            Message::user().with_text(text),
            Message::assistant().with_text("Noted."),
        ]));
        s
    }

    /// A completer that writes one knowledge page and then stops — enough for
    /// the ingest macro to commit, so the ALLOWED direction of every row below
    /// is a real success and not merely "a different error".
    struct WritingCompleter {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl WritingCompleter {
        fn new() -> Self {
            Self {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl Completer for WritingCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[biorouter_mcp::knowledge::subagent::loop_::LlmMessage],
            _tools: &[rmcp::model::Tool],
        ) -> anyhow::Result<biorouter_mcp::knowledge::subagent::loop_::LlmReply> {
            use biorouter_mcp::knowledge::subagent::loop_::{LlmReply, LlmToolCall};
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                return Ok(LlmReply {
                    text: String::new(),
                    tool_calls: vec![LlmToolCall {
                        id: "req-1".into(),
                        name: "kb_write_page".into(),
                        args: serde_json::json!({
                            "path": "knowledge/sources/chat.md",
                            "content": "---\ntitle: Chat\nkind: source\n---\n\nA digest.",
                            "commit_message": "add chat digest"
                        }),
                    }],
                });
            }
            Ok(LlmReply {
                text: "Done.".into(),
                tool_calls: Vec::new(),
            })
        }
    }

    /// Every byte under a knowledge root, so "the barrier wrote nothing" is an
    /// assertion about the tree and not about a return value. The sink is a
    /// machine-wide directory every other session can read, so a refusal that
    /// still wrote is not a refusal.
    fn tree_snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
        fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, base, out);
                } else if let Ok(bytes) = std::fs::read(&p) {
                    let rel = p
                        .strip_prefix(base)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .into_owned();
                    out.push((rel, bytes));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    fn kb_service() -> (tempfile::TempDir, KnowledgeService) {
        let tmp = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("default", "Default", None).unwrap();
        (tmp, svc)
    }

    #[tokio::test]
    async fn ingesting_another_sessions_conversation_obeys_the_barrier() {
        let (tmp, svc) = kb_service();
        let private = session_with("other", SessionClassification::Private, "PHI cohort notes");

        let before = tree_snapshot(tmp.path());
        let err = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: "default".into(),
                caller_capability: ProviderTier::Public,
                sessions: vec![private],
                completer: Box::new(WritingCompleter::new()),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect_err("a public model read another session's private transcript")
        .to_string();

        assert!(err.contains("private"), "must explain: {err}");
        assert!(
            !err.contains("PHI cohort notes"),
            "leaked content into the result: {err}"
        );
        assert!(
            !err.contains("other") && !err.contains("chat other"),
            "the refusal named the session (§11.4 classifies id/title as content): {err}"
        );
        // The strong half: a refusal that still wrote is not a refusal.
        assert_eq!(
            tree_snapshot(tmp.path()),
            before,
            "knowledge base was modified"
        );
    }

    #[tokio::test]
    async fn ingesting_your_own_private_conversation_ratchets_the_knowledge_base() {
        // ⚠ The default (no `session_ids`) is the current session, and that call
        // is the overwhelmingly common one — it must keep working. What it must
        // NOT do is leave a private transcript in a base that stays public.
        let (tmp, svc) = kb_service();
        assert!(
            !crate::knowledge::tier::is_private(tmp.path(), "default"),
            "fixture precondition"
        );

        let out = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: "default".into(),
                caller_capability: ProviderTier::Private,
                sessions: vec![session_with(
                    "mine",
                    SessionClassification::Private,
                    "my own notes",
                )],
                completer: Box::new(WritingCompleter::new()),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect("a private model was refused its own private chat");

        assert_eq!(out.refused, 0);
        assert!(!out.ingested.commit_sha.is_empty());
        // The base now carries the tier of the sessions that fed it, so a public
        // model can no longer read it back — the laundering path is closed.
        assert!(
            crate::knowledge::tier::is_private(tmp.path(), "default"),
            "a private transcript landed in a base that stayed public"
        );
    }

    #[tokio::test]
    async fn a_public_session_may_still_ingest_its_own_conversation() {
        // The other half of "must not regress": the ratchet is `max`, not `set`,
        // so a public chat ingesting itself leaves a public base public.
        let (tmp, svc) = kb_service();
        let out = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: "default".into(),
                caller_capability: ProviderTier::Public,
                sessions: vec![session_with(
                    "mine",
                    SessionClassification::Public,
                    "weekly notes",
                )],
                completer: Box::new(WritingCompleter::new()),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect("a public chat may always ingest itself");

        assert_eq!(out.refused, 0);
        assert!(!crate::knowledge::tier::is_private(tmp.path(), "default"));
    }

    #[tokio::test]
    async fn a_mixed_request_drops_the_private_chats_and_keeps_the_rest() {
        // Per session, not once: `sessions` is a caller-supplied LIST, and a
        // single up-front check on the first element admits the rest.
        let (tmp, svc) = kb_service();
        let out = ingest_conversation(
            &svc,
            ConversationIngestArgs {
                kb_id: "default".into(),
                caller_capability: ProviderTier::Public,
                sessions: vec![
                    session_with("pub", SessionClassification::Public, "PUBLIC-SENTINEL"),
                    session_with("priv", SessionClassification::Private, "PHI cohort notes"),
                ],
                completer: Box::new(WritingCompleter::new()),
                focus: None,
                bounds: SubAgentBounds::default(),
                event_sink: None,
                cancel: None,
            },
        )
        .await
        .expect("the public chat in the list is still ingestible");

        assert_eq!(out.refused, 1, "the private chat must be refused");
        // And the refused transcript reached no byte of the tree — `raw/` holds
        // the rendered markdown verbatim, so this is the honest place to look.
        let written = tree_snapshot(tmp.path())
            .into_iter()
            .map(|(_, b)| String::from_utf8_lossy(&b).into_owned())
            .collect::<String>();
        assert!(
            written.contains("PUBLIC-SENTINEL"),
            "the allowed transcript was dropped too"
        );
        assert!(
            !written.contains("PHI cohort notes"),
            "the refused transcript was rendered into the knowledge base"
        );
    }
}
