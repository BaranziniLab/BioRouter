//! Coalesce the agent's streaming reply events for terminal rendering.
//!
//! Streaming providers emit assistant text one token at a time: each chunk
//! arrives as its own `AgentEvent::Message` carrying only the new fragment
//! (all sharing the response's `id`; see
//! `providers::formats::openai::response_to_streaming_message`). The CLI
//! renders whole messages as Markdown blocks, so feeding it raw per-token
//! deltas shatters every structure — a table's `|---|` separator becomes a
//! horizontal rule, each cell lands on its own line, prose wraps one word per
//! line.
//!
//! This adapter merges consecutive pure-assistant-text deltas of the same
//! response into a single `Message`, emitting it only once the run completes
//! (the next non-text event, a new response id, or end of stream). Every other
//! event passes through untouched and in order, with any buffered text flushed
//! first so ordering is preserved. The trade-off is that assistant text now
//! appears once it is complete rather than token-by-token; the spinner already
//! signals progress while it streams.

use std::collections::VecDeque;

use anyhow::Result;
use biorouter::agents::AgentEvent;
use biorouter::conversation::message::{Message, MessageContent};
use futures::stream::{self, BoxStream, StreamExt};
use rmcp::model::Role;

/// A message is a coalescable streaming text delta when it is assistant-authored
/// and carries nothing but text (no tool calls, thinking, notifications, …).
fn is_pure_assistant_text(m: &Message) -> bool {
    m.role == Role::Assistant
        && !m.content.is_empty()
        && m.content
            .iter()
            .all(|c| matches!(c, MessageContent::Text(_)))
}

/// Two text deltas belong to the same streaming response when their ids match.
/// Streaming chunks all carry the response id; if either lacks one we assume
/// the in-progress run continues (a single non-streaming message is buffered
/// alone and flushed at end-of-stream either way).
fn same_response(pending: &Message, next: &Message) -> bool {
    match (&pending.id, &next.id) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// Normalize a delta into the buffer message: a single text content block so
/// the renderer sees one contiguous Markdown document. Preserves id/metadata.
fn into_pending(mut m: Message) -> Message {
    let text = m.as_concat_text();
    m.content = vec![MessageContent::text(text)];
    m
}

fn append_delta(pending: &mut Message, next: &Message) {
    let extra = next.as_concat_text();
    if let Some(MessageContent::Text(t)) = pending.content.first_mut() {
        t.text.push_str(&extra);
    }
}

struct State<'a> {
    inner: BoxStream<'a, Result<AgentEvent>>,
    pending: Option<Message>,
    outbox: VecDeque<Result<AgentEvent>>,
    inner_done: bool,
}

/// Wrap an agent reply stream so consecutive assistant text deltas are merged
/// into whole messages before reaching the renderer.
pub fn coalesce_text_deltas(
    inner: BoxStream<'_, Result<AgentEvent>>,
) -> BoxStream<'_, Result<AgentEvent>> {
    let state = State {
        inner,
        pending: None,
        outbox: VecDeque::new(),
        inner_done: false,
    };

    Box::pin(stream::unfold(state, |mut st| async move {
        loop {
            if let Some(item) = st.outbox.pop_front() {
                return Some((item, st));
            }
            if st.inner_done {
                return st
                    .pending
                    .take()
                    .map(|p| (Ok(AgentEvent::Message(p)), st));
            }

            match st.inner.next().await {
                None => {
                    st.inner_done = true;
                    // Next loop turn flushes any trailing pending text.
                }
                Some(Ok(AgentEvent::Message(m))) if is_pure_assistant_text(&m) => {
                    match st.pending.as_mut() {
                        Some(p) if same_response(p, &m) => append_delta(p, &m),
                        _ => {
                            if let Some(p) = st.pending.take() {
                                st.outbox.push_back(Ok(AgentEvent::Message(p)));
                            }
                            st.pending = Some(into_pending(m));
                        }
                    }
                }
                Some(other) => {
                    // Any non-text event (or an error): flush buffered text
                    // first so it renders before what follows, then pass through.
                    if let Some(p) = st.pending.take() {
                        st.outbox.push_back(Ok(AgentEvent::Message(p)));
                    }
                    st.outbox.push_back(other);
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_delta(id: &str, s: &str) -> Result<AgentEvent> {
        Ok(AgentEvent::Message(
            Message::assistant().with_id(id.to_string()).with_text(s),
        ))
    }

    async fn collect_text(events: Vec<Result<AgentEvent>>) -> Vec<String> {
        let inner = stream::iter(events).boxed();
        coalesce_text_deltas(inner)
            .filter_map(|e| async move {
                match e {
                    Ok(AgentEvent::Message(m)) if m.role == Role::Assistant => {
                        Some(m.as_concat_text())
                    }
                    _ => None,
                }
            })
            .collect()
            .await
    }

    #[tokio::test]
    async fn merges_same_id_text_deltas_into_one_message() {
        let out = collect_text(vec![
            text_delta("r1", "| a "),
            text_delta("r1", "| b |"),
            text_delta("r1", "\n|---|"),
        ])
        .await;
        assert_eq!(out, vec!["| a | b |\n|---|".to_string()]);
    }

    #[tokio::test]
    async fn different_ids_stay_separate() {
        let out = collect_text(vec![
            text_delta("r1", "hello"),
            text_delta("r2", "world"),
        ])
        .await;
        assert_eq!(out, vec!["hello".to_string(), "world".to_string()]);
    }

    #[tokio::test]
    async fn non_text_event_flushes_pending_first_and_preserves_order() {
        let inner = stream::iter(vec![
            text_delta("r1", "before"),
            Ok(AgentEvent::ModelChange {
                model: "m".into(),
                mode: "lead".into(),
            }),
            text_delta("r2", "after"),
        ])
        .boxed();

        let kinds: Vec<&'static str> = coalesce_text_deltas(inner)
            .map(|e| match e {
                Ok(AgentEvent::Message(_)) => "msg",
                Ok(AgentEvent::ModelChange { .. }) => "model",
                _ => "other",
            })
            .collect()
            .await;
        assert_eq!(kinds, vec!["msg", "model", "msg"]);
    }

    #[tokio::test]
    async fn single_complete_message_passes_through_once() {
        let out = collect_text(vec![text_delta("r1", "one whole message")]).await;
        assert_eq!(out, vec!["one whole message".to_string()]);
    }
}
