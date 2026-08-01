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

/// Render the selected session(s) and ingest them into `kb_id` as one source,
/// reusing the standard knowledge ingest macro.
pub async fn ingest_conversation(
    svc: &KnowledgeService,
    args: ConversationIngestArgs,
) -> anyhow::Result<IngestResult> {
    if args.sessions.is_empty() {
        anyhow::bail!("no conversations selected for ingestion");
    }
    let rendered = render_conversations(&args.sessions);
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
    ingest(
        svc,
        IngestArgs {
            kb_id: args.kb_id,
            // Issue #56. The ProviderTier -> bool crossing, and the only one:
            // `IngestArgs` lives in biorouter-mcp, which cannot name
            // ProviderTier. Task 11 adds the refusal that reads the same field.
            caller_is_private: args.caller_capability.is_private(),
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
    .await
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
}
