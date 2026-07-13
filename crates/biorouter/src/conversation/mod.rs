use crate::conversation::message::{Message, MessageContent, MessageMetadata};
use rmcp::model::Role;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use utoipa::ToSchema;

pub mod message;
pub mod normalize;
pub mod tool_preview;
pub mod tool_result_serde;

pub use normalize::{ConversationNormalizer, SharedNormalizer};

/// The message history of one session.
///
/// BR-56: the transcript is held behind an `Arc` and mutated copy-on-write
/// (`Arc::make_mut`), so `clone()` is a refcount bump rather than a deep copy of
/// every message. The agent loop clones the conversation several times per turn
/// (to hand it to `fix_conversation`, to MOIM injection, to the compaction
/// check, to the reply route's stream task); with a plain `Vec<Message>` each of
/// those copied every tool result in the session — the single largest per-turn
/// allocation in a long session. Only a writer (`push`/`extend`/`pop`/…) pays for
/// a copy, and only while another handle is alive.
#[derive(Debug, Clone, ToSchema, PartialEq)]
pub struct Conversation(#[schema(value_type = Vec<Message>)] Arc<Vec<Message>>);

impl Serialize for Conversation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Conversation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<Message>::deserialize(deserializer).map(|messages| Self(Arc::new(messages)))
    }
}

#[derive(Error, Debug)]
#[error("invalid conversation: {reason}")]
pub struct InvalidConversation {
    reason: String,
    conversation: Conversation,
}

impl Conversation {
    pub fn new<I>(messages: I) -> Result<Self, InvalidConversation>
    where
        I: IntoIterator<Item = Message>,
    {
        Self::new_unvalidated(messages).validate()
    }

    pub fn new_unvalidated<I>(messages: I) -> Self
    where
        I: IntoIterator<Item = Message>,
    {
        Self(Arc::new(messages.into_iter().collect()))
    }

    pub fn empty() -> Self {
        Self::new_unvalidated([])
    }

    pub fn messages(&self) -> &Vec<Message> {
        &self.0
    }

    /// Take ownership of the messages, avoiding a copy when this is the only
    /// handle to the transcript (BR-56). A caller that still holds another clone
    /// pays for the copy-on-write here, exactly as `Arc::make_mut` would.
    pub fn into_messages(self) -> Vec<Message> {
        Arc::try_unwrap(self.0).unwrap_or_else(|shared| shared.as_ref().clone())
    }

    /// The copy-on-write handle used by every mutating method.
    fn messages_mut(&mut self) -> &mut Vec<Message> {
        Arc::make_mut(&mut self.0)
    }

    pub fn push(&mut self, message: Message) {
        let messages = self.messages_mut();
        if let Some(last) = messages
            .last_mut()
            .filter(|m| m.id.is_some() && m.id == message.id)
        {
            match (last.content.last_mut(), message.content.last()) {
                (Some(MessageContent::Text(ref mut last)), Some(MessageContent::Text(new)))
                    if message.content.len() == 1 =>
                {
                    last.text.push_str(&new.text);
                }
                (_, _) => {
                    last.content.extend(message.content);
                }
            }
        } else {
            messages.push(message);
        }
    }

    pub fn last(&self) -> Option<&Message> {
        self.0.last()
    }

    pub fn first(&self) -> Option<&Message> {
        self.0.first()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Message>,
    {
        for message in iter {
            self.push(message);
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Message> {
        self.0.iter()
    }

    pub fn pop(&mut self) -> Option<Message> {
        self.messages_mut().pop()
    }

    pub fn truncate(&mut self, len: usize) {
        self.messages_mut().truncate(len);
    }

    pub fn clear(&mut self) {
        self.messages_mut().clear();
    }

    pub fn filtered_messages<F>(&self, filter: F) -> Vec<Message>
    where
        F: Fn(&MessageMetadata) -> bool,
    {
        self.0
            .iter()
            .filter(|msg| filter(&msg.metadata))
            .cloned()
            .collect()
    }

    pub fn agent_visible_messages(&self) -> Vec<Message> {
        self.filtered_messages(|meta| meta.agent_visible)
    }

    pub fn user_visible_messages(&self) -> Vec<Message> {
        self.filtered_messages(|meta| meta.user_visible)
    }

    fn validate(self) -> Result<Self, InvalidConversation> {
        let (_messages, issues) = fix_messages(self.0.as_ref().clone());
        if !issues.is_empty() {
            let reason = issues.join("\n");
            Err(InvalidConversation {
                reason,
                conversation: self,
            })
        } else {
            Ok(self)
        }
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::empty()
    }
}

impl IntoIterator for Conversation {
    type Item = Message;
    type IntoIter = std::vec::IntoIter<Message>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_messages().into_iter()
    }
}
impl<'a> IntoIterator for &'a Conversation {
    type Item = &'a Message;
    type IntoIter = std::slice::Iter<'a, Message>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A message's place in the original conversation: either the *n*th agent-visible
/// message (normalization may rewrite it) or a non-visible message that passes
/// through untouched. Shared by [`fix_conversation`] and the incremental
/// normalizer (BR-56), which caches the shadow map of the frozen prefix.
#[derive(Debug, Clone)]
pub(crate) enum MessageSlot {
    /// Index into the *input* agent-visible message list.
    Visible(usize),
    /// Non-visible messages pass through unchanged.
    NonVisible(Message),
}

/// Split a message list into its shadow map and the agent-visible messages the
/// fix pipeline actually operates on. Moves rather than clones (BR-56).
pub(crate) fn split_slots(messages: Vec<Message>) -> (Vec<MessageSlot>, Vec<Message>) {
    let mut agent_visible = Vec::new();
    let mut slots = Vec::with_capacity(messages.len());
    for message in messages {
        if message.metadata.agent_visible {
            slots.push(MessageSlot::Visible(agent_visible.len()));
            agent_visible.push(message);
        } else {
            slots.push(MessageSlot::NonVisible(message));
        }
    }
    (slots, agent_visible)
}

/// Rebuild the full message list from the shadow map and the *fixed* visible
/// messages. A visible slot whose index no longer exists (the pipeline removed
/// or merged messages, so the fixed list is shorter) drops out.
pub(crate) fn reassemble(slots: Vec<MessageSlot>, fixed_visible: Vec<Message>) -> Vec<Message> {
    let mut fixed: Vec<Option<Message>> = fixed_visible.into_iter().map(Some).collect();
    slots
        .into_iter()
        .filter_map(|slot| match slot {
            // Slot indices are unique, so taking (rather than cloning) is
            // equivalent — and never copies a message body.
            MessageSlot::Visible(idx) => fixed.get_mut(idx).and_then(Option::take),
            MessageSlot::NonVisible(msg) => Some(msg),
        })
        .collect()
}

/// Fix a conversation that we're about to send to an LLM. So the last and first
/// messages should always be from the user.
pub fn fix_conversation(conversation: Conversation) -> (Conversation, Vec<String>) {
    let (slots, agent_visible_messages) = split_slots(conversation.into_messages());
    let (fixed_visible, issues) = fix_messages(agent_visible_messages);
    (
        Conversation::new_unvalidated(reassemble(slots, fixed_visible)),
        issues,
    )
}

fn fix_messages(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let (messages, mut issues) = fix_messages_core(messages);
    let (messages, edge_issues) = fix_messages_edges(messages);
    issues.extend(edge_issues);
    (messages, issues)
}

/// The body passes. Every one of them is *segment-decomposable* given a clean cut
/// (no tool request in the prefix answered in the suffix): running them over
/// `prefix ++ suffix` gives the same result as running them over each half and
/// re-running [`merge_consecutive_messages`] over the join. That is what lets the
/// incremental normalizer (BR-56) skip the prefix entirely.
fn fix_messages_core(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    run_passes(
        messages,
        [
            merge_text_content_items,
            trim_assistant_text_whitespace,
            remove_empty_messages,
            fix_tool_calling,
            merge_consecutive_messages,
        ],
    )
}

/// The end passes: they only look at the head and the tail of the whole list, so
/// they must run once, over the *complete* message list.
fn fix_messages_edges(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    run_passes(messages, [fix_lead_trail, populate_if_empty])
}

/// One normalization pass: message list in, fixed list + issues out.
type FixPass = fn(Vec<Message>) -> (Vec<Message>, Vec<String>);

fn run_passes<const N: usize>(
    messages: Vec<Message>,
    passes: [FixPass; N],
) -> (Vec<Message>, Vec<String>) {
    passes.into_iter().fold(
        (messages, Vec::new()),
        |(msgs, mut all_issues), processor| {
            let (new_msgs, issues) = processor(msgs);
            all_issues.extend(issues);
            (new_msgs, all_issues)
        },
    )
}

fn merge_text_content_in_message(mut msg: Message) -> Message {
    if msg.role != Role::Assistant {
        return msg;
    }
    msg.content = msg
        .content
        .into_iter()
        .fold(Vec::new(), |mut content, item| {
            match item {
                MessageContent::Text(text) => {
                    if let Some(MessageContent::Text(ref mut last)) = content.last_mut() {
                        last.text.push_str(&text.text);
                    } else {
                        content.push(MessageContent::Text(text));
                    }
                }
                other => content.push(other),
            }
            content
        });
    msg
}

fn merge_text_content_items(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    messages.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut messages, mut issues), message| {
            let content_len = message.content.len();
            let message = merge_text_content_in_message(message);
            if content_len != message.content.len() {
                issues.push(String::from("Merged text content"))
            }
            messages.push(message);
            (messages, issues)
        },
    )
}

fn trim_assistant_text_whitespace(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();

    let fixed_messages = messages
        .into_iter()
        .map(|mut message| {
            if message.role == Role::Assistant {
                for content in &mut message.content {
                    if let MessageContent::Text(text) = content {
                        let trimmed = text.text.trim_end();
                        if trimmed.len() != text.text.len() {
                            issues.push(
                                "Trimmed trailing whitespace from assistant message".to_string(),
                            );
                            text.text = trimmed.to_string();
                        }
                    }
                }
            }
            message
        })
        .collect();

    (fixed_messages, issues)
}

fn remove_empty_messages(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();
    let filtered_messages = messages
        .into_iter()
        .filter(|msg| {
            if msg
                .content
                .iter()
                .all(|c| c.as_text().is_some_and(str::is_empty))
            {
                issues.push("Removed empty message".to_string());
                false
            } else {
                true
            }
        })
        .collect();
    (filtered_messages, issues)
}

fn fix_tool_calling(mut messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();
    let mut pending_tool_requests: HashSet<String> = HashSet::new();

    for message in &mut messages {
        let mut content_to_remove = Vec::new();

        match message.role {
            Role::User => {
                for (idx, content) in message.content.iter().enumerate() {
                    match content {
                        MessageContent::ToolRequest(req) => {
                            content_to_remove.push(idx);
                            issues.push(format!(
                                "Removed tool request '{}' from user message",
                                req.id
                            ));
                        }
                        MessageContent::ToolConfirmationRequest(req) => {
                            content_to_remove.push(idx);
                            issues.push(format!(
                                "Removed tool confirmation request '{}' from user message",
                                req.id
                            ));
                        }
                        MessageContent::Thinking(_) | MessageContent::RedactedThinking(_) => {
                            content_to_remove.push(idx);
                            issues.push("Removed thinking content from user message".to_string());
                        }
                        MessageContent::ToolResponse(resp) => {
                            if pending_tool_requests.contains(&resp.id) {
                                pending_tool_requests.remove(&resp.id);
                            } else {
                                content_to_remove.push(idx);
                                issues
                                    .push(format!("Removed orphaned tool response '{}'", resp.id));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Role::Assistant => {
                for (idx, content) in message.content.iter().enumerate() {
                    match content {
                        MessageContent::ToolResponse(resp) => {
                            content_to_remove.push(idx);
                            issues.push(format!(
                                "Removed tool response '{}' from assistant message",
                                resp.id
                            ));
                        }
                        MessageContent::FrontendToolRequest(req) => {
                            content_to_remove.push(idx);
                            issues.push(format!(
                                "Removed frontend tool request '{}' from assistant message",
                                req.id
                            ));
                        }
                        MessageContent::ToolRequest(req) => {
                            pending_tool_requests.insert(req.id.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        for &idx in content_to_remove.iter().rev() {
            message.content.remove(idx);
        }
    }

    for message in &mut messages {
        if message.role == Role::Assistant {
            let mut content_to_remove = Vec::new();
            for (idx, content) in message.content.iter().enumerate() {
                if let MessageContent::ToolRequest(req) = content {
                    if pending_tool_requests.contains(&req.id) {
                        content_to_remove.push(idx);
                        issues.push(format!("Removed orphaned tool request '{}'", req.id));
                    }
                }
            }
            for &idx in content_to_remove.iter().rev() {
                message.content.remove(idx);
            }
        }
    }
    let (messages, empty_removed) = remove_empty_messages(messages);
    issues.extend(empty_removed);
    (messages, issues)
}

pub fn merge_consecutive_messages(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();
    let mut merged_messages: Vec<Message> = Vec::new();

    for message in messages {
        if let Some(last) = merged_messages.last_mut() {
            let effective = effective_role(&message);
            if effective_role(last) == effective {
                last.content.extend(message.content);
                issues.push(format!("Merged consecutive {} messages", effective));
                continue;
            }
        }
        merged_messages.push(message);
    }

    (merged_messages, issues)
}

fn has_tool_response(message: &Message) -> bool {
    message
        .content
        .iter()
        .any(|content| matches!(content, MessageContent::ToolResponse(_)))
}

pub fn effective_role(message: &Message) -> String {
    if message.role == Role::User && has_tool_response(message) {
        "tool".to_string()
    } else {
        match message.role {
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
        }
    }
}

fn fix_lead_trail(mut messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();

    if let Some(first) = messages.first() {
        if first.role == Role::Assistant {
            messages.remove(0);
            issues.push("Removed leading assistant message".to_string());
        }
    }

    if let Some(last) = messages.last() {
        if last.role == Role::Assistant {
            messages.pop();
            issues.push("Removed trailing assistant message".to_string());
        }
    }

    (messages, issues)
}

const PLACEHOLDER_USER_MESSAGE: &str = "Hello";

fn populate_if_empty(mut messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
    let mut issues = Vec::new();

    if messages.is_empty() {
        issues.push("Added placeholder user message to empty conversation".to_string());
        messages.push(Message::user().with_text(PLACEHOLDER_USER_MESSAGE));
    }
    (messages, issues)
}

pub fn debug_conversation_fix(
    messages: &[Message],
    fixed: &[Message],
    issues: &[String],
) -> String {
    let mut output = String::new();

    output.push_str("=== CONVERSATION FIX DEBUG ===\n\n");

    output.push_str("BEFORE:\n");
    for (i, msg) in messages.iter().enumerate() {
        output.push_str(&format!("  [{}] {}\n", i, msg.debug()));
    }

    output.push_str("\nISSUES FOUND:\n");
    if issues.is_empty() {
        output.push_str("  (none)\n");
    } else {
        for issue in issues {
            output.push_str(&format!("  - {}\n", issue));
        }
    }

    output.push_str("\nAFTER:\n");
    for (i, msg) in fixed.iter().enumerate() {
        output.push_str(&format!("  [{}] {}\n", i, msg.debug()));
    }

    output.push_str("\n==============================\n");
    output
}

#[cfg(test)]
mod tests {
    use crate::conversation::message::Message;
    use crate::conversation::{debug_conversation_fix, fix_conversation, Conversation};
    use rmcp::model::{CallToolRequestParams, Role};
    use rmcp::object;

    macro_rules! assert_has_issues_unordered {
        ($fixed:expr, $issues:expr, $($expected:expr),+ $(,)?) => {
            {
                let mut expected: Vec<&str> = vec![$($expected),+];
                let mut actual: Vec<&str> = $issues.iter().map(|s| s.as_str()).collect();
                expected.sort();
                actual.sort();

                if actual != expected {
                    panic!(
                        "assertion failed: issues don't match\nexpected: {:?}\n  actual: {:?}. Fixed conversation is:\n{:#?}",
                        expected, $issues, $fixed,
                    );
                }
            }
        };
    }

    fn run_verify(messages: Vec<Message>) -> (Vec<Message>, Vec<String>) {
        let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages.clone()));

        // Uncomment the following line to print the debug report
        // let report = debug_conversation_fix(&messages, &fixed, &issues);
        // print!("\n{}", report);

        let (_fixed, issues_with_fixed) = fix_conversation(fixed.clone());
        assert_eq!(
            issues_with_fixed.len(),
            0,
            "Fixed conversation should have no issues, but found: {:?}\n\n{}",
            issues_with_fixed,
            debug_conversation_fix(&messages, fixed.messages(), &issues)
        );
        (fixed.messages().clone(), issues)
    }

    #[test]
    fn test_valid_conversation() {
        let all_messages = [
            Message::user().with_text("Can you help me search for something?"),
            Message::assistant()
                .with_text("I'll help you search.")
                .with_tool_request(
                    "search_1",
                    Ok(CallToolRequestParams {
                        task: None,
                        name: "web_search".into(),
                        arguments: Some(object!({"query": "rust programming"})),
                        meta: None,
                    }),
                ),
            Message::user().with_tool_response(
                "search_1",
                Ok(rmcp::model::CallToolResult {
                    content: vec![],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
            ),
            Message::assistant().with_text("Based on the search results, here's what I found..."),
        ];

        for i in 1..=all_messages.len() {
            let messages = Conversation::new_unvalidated(all_messages[..i].to_vec());
            if messages.last().unwrap().role == Role::User {
                let (fixed, issues) = fix_conversation(messages.clone());
                assert_eq!(
                    fixed.len(),
                    messages.len(),
                    "Step {}: Length should match",
                    i
                );
                assert!(
                    issues.is_empty(),
                    "Step {}: Should have no issues, but found: {:?}",
                    i,
                    issues
                );
                assert_eq!(
                    fixed.messages(),
                    messages.messages(),
                    "Step {}: Messages should be unchanged",
                    i
                );
            }
        }
    }

    #[test]
    fn test_role_alternation_and_content_placement_issues() {
        let messages = vec![
            Message::user().with_text("Hello"),
            Message::user().with_text("Another user message"),
            Message::assistant()
                .with_text("Response")
                .with_tool_response(
                    "orphan_1",
                    Ok(rmcp::model::CallToolResult {
                        content: vec![],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    }),
                ), // Wrong role
            Message::assistant().with_thinking("Let me think", "sig"),
            Message::user()
                .with_tool_request(
                    "bad_req",
                    Ok(CallToolRequestParams {
                        task: None,
                        name: "search".into(),
                        arguments: Some(object!({})),
                        meta: None,
                    }),
                )
                .with_text("User with bad tool request"),
        ];

        let (fixed, issues) = run_verify(messages);

        assert_eq!(fixed.len(), 3);

        assert_has_issues_unordered!(
            fixed,
            issues,
            "Merged consecutive assistant messages",
            "Merged consecutive user messages",
            "Removed tool response 'orphan_1' from assistant message",
            "Removed tool request 'bad_req' from user message",
        );

        assert_eq!(fixed[0].role, Role::User);
        assert_eq!(fixed[1].role, Role::Assistant);
        assert_eq!(fixed[2].role, Role::User);

        assert_eq!(fixed[0].content.len(), 2);
    }

    #[test]
    fn test_orphaned_tools_and_empty_messages() {
        // This conversation completely collapses. the first user message is invalid
        // then we remove the empty user message and the wrong tool response
        // then we collapse the assistant messages
        // which we then remove because you can't end a conversation with an assistant message
        let messages = vec![
            Message::assistant()
                .with_text("I'll search for you")
                .with_tool_request(
                    "search_1",
                    Ok(CallToolRequestParams {
                        task: None,
                        name: "search".into(),
                        arguments: Some(object!({})),
                        meta: None,
                    }),
                ),
            Message::user(),
            Message::user().with_tool_response(
                "wrong_id",
                Ok(rmcp::model::CallToolResult {
                    content: vec![],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
            ),
            Message::assistant().with_tool_request(
                "search_2",
                Ok(CallToolRequestParams {
                    task: None,
                    name: "search".into(),
                    arguments: Some(object!({})),
                    meta: None,
                }),
            ),
        ];

        let (fixed, issues) = run_verify(messages);

        assert_eq!(fixed.len(), 1);

        assert_has_issues_unordered!(
            fixed,
            issues,
            "Removed empty message",
            "Removed orphaned tool response 'wrong_id'",
            "Removed orphaned tool request 'search_1'",
            "Removed orphaned tool request 'search_2'",
            "Removed empty message",
            "Removed empty message",
            "Removed leading assistant message",
            "Added placeholder user message to empty conversation",
        );

        assert_eq!(fixed[0].role, Role::User);
        assert_eq!(fixed[0].as_concat_text(), "Hello");
    }

    #[test]
    fn test_real_world_consecutive_assistant_messages() {
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("run ls in the current directory and then run a word count on the smallest file"),

            Message::assistant()
                .with_text("I'll help you run `ls` in the current directory and then perform a word count on the smallest file. Let me start by listing the directory contents.")
                .with_tool_request("toolu_bdrk_018adWbP4X26CfoJU5hkhu3i", Ok(CallToolRequestParams { task: None, name: "developer__shell".into(), arguments: Some(object!({"command": "ls -la"})), meta: None })),

            Message::assistant()
                .with_text("Now I'll identify the smallest file by size. Looking at the output, I can see that both `slack.yaml` and `subworkflows.yaml` have a size of 0 bytes, making them the smallest files. I'll run a word count on one of them:")
                .with_tool_request("toolu_bdrk_01KgDYHs4fAodi22NqxRzmwx", Ok(CallToolRequestParams { task: None, name: "developer__shell".into(), arguments: Some(object!({"command": "wc slack.yaml"})), meta: None })),

            Message::user()
                .with_tool_response("toolu_bdrk_01KgDYHs4fAodi22NqxRzmwx", Ok(rmcp::model::CallToolResult {
                    content: vec![],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                })),

            Message::assistant()
                .with_text("I ran `ls -la` in the current directory and found several files. Looking at the file sizes, I can see that both `slack.yaml` and `subworkflows.yaml` are 0 bytes (the smallest files). I ran a word count on `slack.yaml` which shows: **0 lines**, **0 words**, **0 characters**"),
            Message::user().with_text("thanks!"),
        ]);

        let (fixed, issues) = fix_conversation(conversation);

        assert_eq!(fixed.len(), 5);
        assert_has_issues_unordered!(
            fixed,
            issues,
            "Removed orphaned tool request 'toolu_bdrk_018adWbP4X26CfoJU5hkhu3i'",
            "Merged consecutive assistant messages"
        )
    }

    #[test]
    fn test_tool_response_effective_role() {
        let messages = vec![
            Message::user().with_text("Search for something"),
            Message::assistant()
                .with_text("I'll search for you")
                .with_tool_request(
                    "search_1",
                    Ok(CallToolRequestParams {
                        task: None,
                        name: "search".into(),
                        arguments: Some(object!({})),
                        meta: None,
                    }),
                ),
            Message::user().with_tool_response(
                "search_1",
                Ok(rmcp::model::CallToolResult {
                    content: vec![],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
            ),
            Message::user().with_text("Thanks!"),
        ];

        let (_fixed, issues) = run_verify(messages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_merge_text_content_items() {
        use crate::conversation::message::MessageContent;
        use rmcp::model::{AnnotateAble, RawTextContent};

        let mut message = Message::assistant().with_text("Hello");

        message.content.push(MessageContent::Text(
            RawTextContent {
                text: " world".to_string(),
                meta: None,
            }
            .no_annotation(),
        ));
        message.content.push(MessageContent::Text(
            RawTextContent {
                text: "!".to_string(),
                meta: None,
            }
            .no_annotation(),
        ));

        let messages = vec![
            Message::user().with_text("hello"),
            message,
            Message::user().with_text("thanks"),
        ];

        let (fixed, issues) = run_verify(messages);

        assert_eq!(fixed.len(), 3);
        assert_has_issues_unordered!(fixed, issues, "Merged text content");

        let fixed_msg = &fixed[1];
        assert_eq!(fixed_msg.content.len(), 1);

        if let MessageContent::Text(text_content) = &fixed_msg.content[0] {
            assert_eq!(text_content.text, "Hello world!");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_merge_text_content_items_with_mixed_content() {
        use crate::conversation::message::MessageContent;
        use rmcp::model::{AnnotateAble, RawTextContent};

        let mut image_message = Message::assistant().with_text("Look at");

        image_message.content.push(MessageContent::Text(
            RawTextContent {
                text: " this image:".to_string(),
                meta: None,
            }
            .no_annotation(),
        ));

        image_message = image_message.with_image("", "");

        let messages = vec![
            Message::user().with_text("hello"),
            image_message,
            Message::user().with_text("thanks"),
        ];

        let (fixed, issues) = run_verify(messages);

        assert_eq!(fixed.len(), 3);
        assert_has_issues_unordered!(fixed, issues, "Merged text content");
        let fixed_msg = &fixed[1];

        assert_eq!(fixed_msg.content.len(), 2);
        if let MessageContent::Text(text_content) = &fixed_msg.content[0] {
            assert_eq!(text_content.text, "Look at this image:");
        } else {
            panic!("Expected first item to be text content");
        }

        if let MessageContent::Image(_) = &fixed_msg.content[1] {
            // Good
        } else {
            panic!("Expected second item to be an image");
        }
    }

    #[test]
    fn test_agent_visible_non_visible_message_ordering_with_fixes() {
        // Test that non-visible messages maintain their position relative to visible messages
        // even when visible messages are fixed (merged, removed, etc.)

        // Create messages with mixed visibility where visible ones need fixing
        let mut msg1_user = Message::user().with_text("First user message");
        msg1_user.metadata.agent_visible = true;

        let mut msg2_non_visible = Message::user().with_text("Non-visible note 1");
        msg2_non_visible.metadata.agent_visible = false;

        // These two consecutive user messages should be merged (triggering a fix)
        let mut msg3_user = Message::user().with_text("Second user message");
        msg3_user.metadata.agent_visible = true;

        let mut msg4_user = Message::user().with_text("Third user message");
        msg4_user.metadata.agent_visible = true;

        let mut msg5_non_visible = Message::user().with_text("Non-visible note 2");
        msg5_non_visible.metadata.agent_visible = false;

        let mut msg6_assistant = Message::assistant().with_text("Assistant response");
        msg6_assistant.metadata.agent_visible = true;

        let mut msg7_non_visible = Message::user().with_text("Non-visible note 3");
        msg7_non_visible.metadata.agent_visible = false;

        let mut msg8_user = Message::user().with_text("Final user message");
        msg8_user.metadata.agent_visible = true;

        let messages = vec![
            msg1_user.clone(),
            msg2_non_visible.clone(),
            msg3_user.clone(),
            msg4_user.clone(),
            msg5_non_visible.clone(),
            msg6_assistant.clone(),
            msg7_non_visible.clone(),
            msg8_user.clone(),
        ];

        let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages.clone()));

        // Should have merged consecutive user messages
        assert!(!issues.is_empty());
        assert!(issues.iter().any(|i| i.contains("Merged consecutive")));

        let fixed_messages = fixed.messages();

        // Verify non-visible messages are still present
        let non_visible_texts: Vec<String> = fixed_messages
            .iter()
            .filter(|m| !m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(non_visible_texts.len(), 3);
        assert_eq!(non_visible_texts[0], "Non-visible note 1");
        assert_eq!(non_visible_texts[1], "Non-visible note 2");
        assert_eq!(non_visible_texts[2], "Non-visible note 3");

        // Verify visible messages were processed
        let visible_texts: Vec<String> = fixed_messages
            .iter()
            .filter(|m| m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        // Should have 3 visible messages: first user, merged user messages, assistant, final user
        // But after merging consecutive users and fixing lead/trail, we get fewer
        assert!(!visible_texts.is_empty());

        // The key assertion: non-visible messages should be preserved and not reordered
        // relative to each other
        let mut found_note1 = false;
        let mut found_note2 = false;

        for msg in fixed_messages {
            let text = msg.as_concat_text();
            if text == "Non-visible note 1" {
                assert!(!found_note2 && !found_note1);
                found_note1 = true;
            } else if text == "Non-visible note 2" {
                assert!(found_note1 && !found_note2);
                found_note2 = true;
            } else if text == "Non-visible note 3" {
                assert!(found_note1 && found_note2);
            }
        }
    }

    #[test]
    fn test_shadow_map_with_multiple_consecutive_merges() {
        // Test the shadow map handles multiple consecutive visible messages that all merge
        let mut msg1 = Message::user().with_text("User 1");
        msg1.metadata.agent_visible = true;

        let mut msg2_non_vis = Message::user().with_text("Non-visible A");
        msg2_non_vis.metadata.agent_visible = false;

        let mut msg3 = Message::user().with_text("User 2");
        msg3.metadata.agent_visible = true;

        let mut msg4 = Message::user().with_text("User 3");
        msg4.metadata.agent_visible = true;

        let mut msg5 = Message::user().with_text("User 4");
        msg5.metadata.agent_visible = true;

        let mut msg6_non_vis = Message::user().with_text("Non-visible B");
        msg6_non_vis.metadata.agent_visible = false;

        let messages = vec![
            msg1,
            msg2_non_vis.clone(),
            msg3,
            msg4,
            msg5,
            msg6_non_vis.clone(),
        ];

        let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages));

        // Should have merged the consecutive user messages
        assert!(issues.iter().any(|i| i.contains("Merged consecutive")));

        let fixed_messages = fixed.messages();

        // Non-visible messages should still be present and in order
        let non_visible: Vec<String> = fixed_messages
            .iter()
            .filter(|m| !m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(non_visible.len(), 2);
        assert_eq!(non_visible[0], "Non-visible A");
        assert_eq!(non_visible[1], "Non-visible B");

        // The merged message should contain all the user texts
        let visible: Vec<String> = fixed_messages
            .iter()
            .filter(|m| m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(visible.len(), 1);
        assert!(visible[0].contains("User 1"));
        assert!(visible[0].contains("User 2"));
        assert!(visible[0].contains("User 3"));
        assert!(visible[0].contains("User 4"));
    }

    #[test]
    fn test_shadow_map_with_leading_trailing_removal() {
        // Test that shadow map handles removal of leading/trailing assistant messages
        let mut msg1_assistant = Message::assistant().with_text("Leading assistant");
        msg1_assistant.metadata.agent_visible = true;

        let mut msg2_non_vis = Message::user().with_text("Non-visible note");
        msg2_non_vis.metadata.agent_visible = false;

        let mut msg3_user = Message::user().with_text("User message");
        msg3_user.metadata.agent_visible = true;

        let mut msg4_assistant = Message::assistant().with_text("Assistant response");
        msg4_assistant.metadata.agent_visible = true;

        let mut msg5_assistant = Message::assistant().with_text("Trailing assistant");
        msg5_assistant.metadata.agent_visible = true;

        let messages = vec![
            msg1_assistant,
            msg2_non_vis.clone(),
            msg3_user,
            msg4_assistant,
            msg5_assistant,
        ];

        let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages));

        // Should have merged consecutive assistants, removed leading, and removed trailing
        assert!(issues
            .iter()
            .any(|i| i.contains("Merged consecutive assistant")));
        assert!(issues
            .iter()
            .any(|i| i.contains("Removed leading assistant")));
        assert!(issues
            .iter()
            .any(|i| i.contains("Removed trailing assistant")));

        let fixed_messages = fixed.messages();

        // Non-visible message should still be present
        let non_visible: Vec<String> = fixed_messages
            .iter()
            .filter(|m| !m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(non_visible.len(), 1);
        assert_eq!(non_visible[0], "Non-visible note");

        // The two consecutive assistant messages get merged, then the merged message
        // is removed as trailing, leaving only the user message
        let visible: Vec<String> = fixed_messages
            .iter()
            .filter(|m| m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0], "User message");
    }

    #[test]
    fn test_shadow_map_all_visible_messages_removed() {
        // Edge case: all visible messages are removed, only non-visible remain
        let mut msg1_assistant = Message::assistant().with_text("Only assistant");
        msg1_assistant.metadata.agent_visible = true;

        let mut msg2_non_vis = Message::user().with_text("Non-visible note 1");
        msg2_non_vis.metadata.agent_visible = false;

        let mut msg3_non_vis = Message::user().with_text("Non-visible note 2");
        msg3_non_vis.metadata.agent_visible = false;

        let messages = vec![msg1_assistant, msg2_non_vis, msg3_non_vis];

        let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages));

        // Should have removed the assistant and added placeholder
        assert!(issues
            .iter()
            .any(|i| i.contains("Removed leading assistant")));
        assert!(issues.iter().any(|i| i.contains("Added placeholder")));

        let fixed_messages = fixed.messages();

        // Non-visible messages should still be present
        let non_visible: Vec<String> = fixed_messages
            .iter()
            .filter(|m| !m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(non_visible.len(), 2);
        assert_eq!(non_visible[0], "Non-visible note 1");
        assert_eq!(non_visible[1], "Non-visible note 2");

        // Should have placeholder user message
        let visible: Vec<String> = fixed_messages
            .iter()
            .filter(|m| m.metadata.agent_visible)
            .map(|m| m.as_concat_text())
            .collect();

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0], "Hello");
    }

    #[test]
    fn test_shadow_map_preserves_interleaving_pattern() {
        // Test that complex interleaving patterns are preserved
        let mut msg1_user = Message::user().with_text("User 1");
        msg1_user.metadata.agent_visible = true;

        let mut msg2_non_vis = Message::user().with_text("Non-vis A");
        msg2_non_vis.metadata.agent_visible = false;

        let mut msg3_assistant = Message::assistant().with_text("Assistant 1");
        msg3_assistant.metadata.agent_visible = true;

        let mut msg4_non_vis = Message::user().with_text("Non-vis B");
        msg4_non_vis.metadata.agent_visible = false;

        let mut msg5_user = Message::user().with_text("User 2");
        msg5_user.metadata.agent_visible = true;

        let mut msg6_non_vis = Message::user().with_text("Non-vis C");
        msg6_non_vis.metadata.agent_visible = false;

        let messages = vec![
            msg1_user,
            msg2_non_vis,
            msg3_assistant,
            msg4_non_vis,
            msg5_user,
            msg6_non_vis,
        ];

        let (fixed, issues) = fix_conversation(Conversation::new_unvalidated(messages));

        // Should have no issues for this valid conversation
        assert!(issues.is_empty());

        let fixed_messages = fixed.messages();

        // Verify the interleaving pattern is preserved
        assert_eq!(fixed_messages.len(), 6);

        assert_eq!(fixed_messages[0].as_concat_text(), "User 1");
        assert!(fixed_messages[0].metadata.agent_visible);

        assert_eq!(fixed_messages[1].as_concat_text(), "Non-vis A");
        assert!(!fixed_messages[1].metadata.agent_visible);

        assert_eq!(fixed_messages[2].as_concat_text(), "Assistant 1");
        assert!(fixed_messages[2].metadata.agent_visible);

        assert_eq!(fixed_messages[3].as_concat_text(), "Non-vis B");
        assert!(!fixed_messages[3].metadata.agent_visible);

        assert_eq!(fixed_messages[4].as_concat_text(), "User 2");
        assert!(fixed_messages[4].metadata.agent_visible);

        assert_eq!(fixed_messages[5].as_concat_text(), "Non-vis C");
        assert!(!fixed_messages[5].metadata.agent_visible);
    }
}
