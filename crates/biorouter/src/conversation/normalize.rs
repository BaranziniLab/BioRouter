//! Incremental conversation normalization (BR-56).
//!
//! `fix_conversation` runs a seven-pass normalization over the *entire* history,
//! and the agent runs it at least once per reply — plus once more per provider
//! call, inside MOIM injection. In a long session that is O(history) work on
//! every turn, for a transcript whose prefix has not changed since the last turn
//! and cannot change: it was already normalized, and normalization is idempotent.
//!
//! [`ConversationNormalizer`] caches the normalized *prefix* and re-runs the
//! pipeline only over the suffix appended since the last call, turning per-turn
//! normalization from O(history) into O(delta).
//!
//! ## Why this is safe
//!
//! The pipeline is split into *body* passes ([`fix_messages_core`]) and *end*
//! passes ([`fix_messages_edges`]). The body passes are segment-decomposable:
//!
//! * `merge_text_content_items`, `trim_assistant_text_whitespace` and
//!   `remove_empty_messages` are per-message maps/filters — trivially so.
//! * `fix_tool_calling` carries exactly one piece of cross-message state: the set
//!   of tool requests still awaiting a response. The cut is therefore only taken
//!   at a **clean boundary** — a point where no tool request in the prefix is
//!   still open — so the suffix pass starts from the same empty state the full
//!   pass would have, and no response in the suffix is orphaned from a request in
//!   the prefix.
//! * `merge_consecutive_messages` is a left fold that can merge *across* the cut.
//!   It is re-run over the joined list (a cheap move-only pass, and idempotent
//!   inside each already-merged half), which reproduces exactly what the full run
//!   would have done at the seam.
//!
//! The end passes (`fix_lead_trail`, `populate_if_empty`) only ever look at the
//! first and last message, so they always run over the complete joined list.
//!
//! The prefix is only reused when a per-message fingerprint of every input
//! message in it still matches. Anything that rewrites history — compaction
//! (which flips `agent_visible` on old messages and splices in a summary),
//! `HistoryReplaced`, a truncation, a large-response rewrite, a session reload —
//! changes a fingerprint and falls back to a full normalization. A miss costs one
//! ordinary `fix_conversation`; it can never produce a wrong transcript.

use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use ahash::AHasher;
use rmcp::model::Role;

use super::message::{Message, MessageContent};
use super::{
    fix_conversation, fix_messages_core, fix_messages_edges, merge_consecutive_messages,
    reassemble, split_slots, Conversation, MessageSlot,
};
use crate::config::Config;

/// How many trailing input messages are never frozen into the cached prefix.
///
/// The tail of a live conversation churns: MOIM is stripped and re-inserted near
/// the end on every provider call, a streamed assistant message grows in place,
/// tool responses land against their requests. Holding the last few messages out
/// of the frozen prefix keeps the cache stable across those rewrites (and keeps
/// the two call sites — the reply-context fix and the MOIM fix, whose inputs
/// differ only near the end — sharing one frozen prefix).
const TAIL_SLACK: usize = 8;

/// Below this many messages the full pipeline is cheap and the bookkeeping is not
/// worth it.
const MIN_MESSAGES_TO_CACHE: usize = TAIL_SLACK * 2;

fn incremental_enabled() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_INCREMENTAL_NORMALIZE")
        .unwrap_or(true)
}

/// Caches the normalized prefix of one session's transcript.
#[derive(Debug, Default)]
pub struct ConversationNormalizer {
    /// Fingerprint of every *input* message absorbed into the frozen prefix.
    frozen_fingerprints: Vec<u64>,
    /// Shadow map of the frozen prefix (visible slot indices are already global).
    frozen_slots: Vec<MessageSlot>,
    /// `fix_messages_core` applied to the frozen prefix's agent-visible messages.
    frozen_visible: Vec<Message>,
    /// Number of agent-visible messages in the frozen *input* prefix, i.e. the
    /// offset to add to the suffix's visible slot indices.
    frozen_visible_input_count: usize,
    /// Issues the frozen prefix produced, replayed on every call so callers see
    /// the same issue set a full `fix_conversation` would have reported.
    frozen_issues: Vec<String>,
    hits: u64,
    misses: u64,
}

impl ConversationNormalizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of calls that reused a frozen prefix / had to normalize in full.
    /// Test + observability hook.
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    fn reset(&mut self) {
        self.frozen_fingerprints.clear();
        self.frozen_slots.clear();
        self.frozen_visible.clear();
        self.frozen_visible_input_count = 0;
        self.frozen_issues.clear();
    }

    /// Normalize `conversation`, reusing the cached prefix when the input still
    /// starts with the messages it was built from. Output is identical to
    /// [`fix_conversation`] (issue *ordering* may differ; the set does not).
    pub fn normalize(&mut self, conversation: Conversation) -> (Conversation, Vec<String>) {
        if !incremental_enabled() {
            self.reset();
            return fix_conversation(conversation);
        }

        let mut messages = conversation.into_messages();
        let fingerprints: Vec<u64> = messages.iter().map(message_fingerprint).collect();

        let frozen_len = self.frozen_fingerprints.len();
        let reusable = frozen_len > 0
            && frozen_len <= messages.len()
            && self.frozen_fingerprints[..] == fingerprints[..frozen_len];

        if reusable {
            self.hits += 1;
        } else {
            self.misses += 1;
            self.reset();
        }
        let frozen_len = self.frozen_fingerprints.len();

        // Everything after the frozen prefix is re-normalized from scratch.
        let suffix = messages.split_off(frozen_len);
        drop(messages);

        // Freeze as much of the suffix as we safely can for the *next* call: up to
        // TAIL_SLACK messages from the end, cut at a point with no open tool call.
        let freeze_upto = clean_cut(&suffix, suffix.len().saturating_sub(TAIL_SLACK));
        let freeze_upto = if frozen_len + suffix.len() < MIN_MESSAGES_TO_CACHE {
            0
        } else {
            freeze_upto
        };

        let mut suffix = suffix;
        let tail = suffix.split_off(freeze_upto);
        let head = suffix;

        let (head_slots, head_visible) = split_slots(head);
        let (tail_slots, tail_visible) = split_slots(tail);
        let head_visible_count = head_visible.len();

        let (head_core, head_issues) = fix_messages_core(head_visible);
        let (tail_core, tail_issues) = fix_messages_core(tail_visible);

        // Slots: frozen + head + tail, with the visible indices rebased onto the
        // global input-visible numbering a full pass would have used.
        let head_offset = self.frozen_visible_input_count;
        let tail_offset = head_offset + head_visible_count;
        let head_slots: Vec<MessageSlot> = head_slots
            .into_iter()
            .map(|slot| rebase(slot, head_offset))
            .collect();

        let mut slots: Vec<MessageSlot> =
            Vec::with_capacity(self.frozen_slots.len() + head_slots.len() + tail_slots.len());
        slots.extend(self.frozen_slots.iter().cloned());
        slots.extend(head_slots.iter().cloned());
        slots.extend(tail_slots.into_iter().map(|slot| rebase(slot, tail_offset)));

        // Joined visible list: the frozen prefix (copied — the caller owns the
        // result) plus the freshly normalized head and tail.
        let mut joined: Vec<Message> =
            Vec::with_capacity(self.frozen_visible.len() + head_core.len() + tail_core.len());
        joined.extend(self.frozen_visible.iter().cloned());
        joined.extend(head_core.iter().cloned());
        joined.extend(tail_core);

        // Re-run the one boundary-sensitive body pass over the seams, then the end
        // passes over the complete list.
        let (joined, seam_issues) = merge_consecutive_messages(joined);
        let (fixed_visible, edge_issues) = fix_messages_edges(joined);

        let mut issues = self.frozen_issues.clone();
        issues.extend(head_issues.iter().cloned());
        issues.extend(tail_issues);
        issues.extend(seam_issues);
        issues.extend(edge_issues);

        let final_messages = reassemble(slots, fixed_visible);

        // Advance the frozen prefix. Merging the two already-merged halves is the
        // same as merging the concatenation (the fold is associative), so the
        // stored prefix stays exactly `fix_messages_core(input[..b])`.
        if freeze_upto > 0 {
            let mut frozen_visible = std::mem::take(&mut self.frozen_visible);
            frozen_visible.extend(head_core);
            let (frozen_visible, join_issues) = merge_consecutive_messages(frozen_visible);
            self.frozen_visible = frozen_visible;
            self.frozen_slots.extend(head_slots);
            self.frozen_visible_input_count += head_visible_count;
            self.frozen_fingerprints
                .extend_from_slice(&fingerprints[frozen_len..frozen_len + freeze_upto]);
            self.frozen_issues.extend(head_issues);
            self.frozen_issues.extend(join_issues);
            debug_assert_eq!(self.frozen_fingerprints.len(), frozen_len + freeze_upto);
        }

        (Conversation::new_unvalidated(final_messages), issues)
    }
}

fn rebase(slot: MessageSlot, offset: usize) -> MessageSlot {
    match slot {
        MessageSlot::Visible(idx) => MessageSlot::Visible(idx + offset),
        other => other,
    }
}

/// The largest cut point `<= limit` at which no agent-visible tool request is
/// still awaiting its response — the only cross-message state `fix_tool_calling`
/// carries. Returns 0 when no such point exists (nothing is frozen).
fn clean_cut(messages: &[Message], limit: usize) -> usize {
    let limit = limit.min(messages.len());
    let mut open: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut best = 0usize;

    for (idx, message) in messages.iter().enumerate().take(limit) {
        if !message.metadata.agent_visible {
            if open.is_empty() {
                best = idx + 1;
            }
            continue;
        }
        for content in &message.content {
            match (&message.role, content) {
                // Mirrors `fix_tool_calling`: only an assistant's requests open,
                // and only a user's responses close.
                (Role::Assistant, MessageContent::ToolRequest(req)) => {
                    open.insert(req.id.as_str());
                }
                (Role::User, MessageContent::ToolResponse(resp)) => {
                    open.remove(resp.id.as_str());
                }
                _ => {}
            }
        }
        if open.is_empty() {
            best = idx + 1;
        }
    }

    best
}

/// A cheap, change-sensitive fingerprint of one message.
///
/// This is a *cache validator*, not a content hash for security: it must change
/// whenever anything the normalization pipeline reads about the message changes —
/// role, visibility metadata, text bodies, tool ids/pairing, and the shape of the
/// content list. It hashes the message body itself (not just a shape summary), so
/// an in-place rewrite (context pruning replacing a tool result, a large-response
/// swap) misses the cache and forces a full re-normalization.
fn message_fingerprint(message: &Message) -> u64 {
    let mut hasher = AHasher::default();
    matches!(message.role, Role::Assistant).hash(&mut hasher);
    message.id.hash(&mut hasher);
    message.created.hash(&mut hasher);
    message.metadata.agent_visible.hash(&mut hasher);
    message.metadata.user_visible.hash(&mut hasher);
    message.content.len().hash(&mut hasher);

    for content in &message.content {
        content_discriminant(content).hash(&mut hasher);
        match content {
            MessageContent::Text(text) => text.text.hash(&mut hasher),
            MessageContent::Thinking(thinking) => {
                thinking.thinking.hash(&mut hasher);
                thinking.signature.hash(&mut hasher);
            }
            MessageContent::RedactedThinking(redacted) => redacted.data.hash(&mut hasher),
            MessageContent::Image(image) => {
                image.mime_type.hash(&mut hasher);
                image.data.hash(&mut hasher);
            }
            MessageContent::ToolRequest(req) => {
                req.id.hash(&mut hasher);
                match &req.tool_call {
                    Ok(call) => {
                        call.name.hash(&mut hasher);
                        call.arguments
                            .as_ref()
                            .map(|args| args.len())
                            .hash(&mut hasher);
                    }
                    Err(err) => err.message.hash(&mut hasher),
                }
            }
            MessageContent::ToolResponse(resp) => {
                resp.id.hash(&mut hasher);
                match &resp.tool_result {
                    Ok(result) => {
                        result.content.len().hash(&mut hasher);
                        for item in result.content.iter() {
                            match item.as_text() {
                                Some(text) => {
                                    1u8.hash(&mut hasher);
                                    text.text.hash(&mut hasher);
                                }
                                None => 0u8.hash(&mut hasher),
                            }
                        }
                    }
                    Err(err) => err.message.hash(&mut hasher),
                }
            }
            // Rare, small, and never load-bearing for the pipeline's cross-message
            // state — the Display form captures everything that identifies them.
            other => other.to_string().hash(&mut hasher),
        }
    }

    hasher.finish()
}

fn content_discriminant(content: &MessageContent) -> u8 {
    match content {
        MessageContent::Text(_) => 0,
        MessageContent::Image(_) => 1,
        MessageContent::ToolRequest(_) => 2,
        MessageContent::ToolResponse(_) => 3,
        MessageContent::ToolConfirmationRequest(_) => 4,
        MessageContent::ActionRequired(_) => 5,
        MessageContent::FrontendToolRequest(_) => 6,
        MessageContent::Thinking(_) => 7,
        MessageContent::RedactedThinking(_) => 8,
        MessageContent::SystemNotification(_) => 9,
    }
}

/// A [`ConversationNormalizer`] usable from `&self` (the agent holds one per
/// instance). The lock is never held across an await.
#[derive(Debug, Default)]
pub struct SharedNormalizer(Mutex<ConversationNormalizer>);

impl SharedNormalizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn normalize(&self, conversation: Conversation) -> (Conversation, Vec<String>) {
        match self.0.lock() {
            Ok(mut normalizer) => normalizer.normalize(conversation),
            // A poisoned lock only means some other thread panicked mid-normalize;
            // the cache may be stale, so fall back to the full (stateless) fix
            // rather than propagating the panic into the agent loop.
            Err(_) => fix_conversation(conversation),
        }
    }

    pub fn stats(&self) -> (u64, u64) {
        self.0
            .lock()
            .map(|normalizer| normalizer.stats())
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content};

    fn user(text: &str) -> Message {
        Message::user().with_text(text)
    }

    fn assistant(text: &str) -> Message {
        Message::assistant().with_text(text)
    }

    fn tool_call(id: &str) -> Message {
        Message::assistant().with_tool_request(
            id,
            Ok(CallToolRequestParams {
                name: "developer__shell".into(),
                arguments: None,
                meta: None,
                task: None,
            }),
        )
    }

    fn tool_reply(id: &str, text: &str) -> Message {
        Message::user().with_tool_response(
            id,
            Ok(CallToolResult {
                content: vec![Content::text(text)],
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
        )
    }

    /// `populate_if_empty` mints a fresh placeholder message stamped with the
    /// current time, so two independent `fix` runs of an all-removed conversation
    /// differ only in that placeholder's `created`. Zero the timestamps before
    /// comparing — real messages carry their `created` through both paths
    /// unchanged, so this only masks the inherently nondeterministic placeholder.
    fn without_timestamps(messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .cloned()
            .map(|mut message| {
                message.created = 0;
                message
            })
            .collect()
    }

    /// Grow a conversation one turn at a time and assert the incremental result is
    /// byte-identical to a from-scratch `fix_conversation` at every step.
    fn assert_matches_full_fix(steps: Vec<Vec<Message>>) {
        let mut normalizer = ConversationNormalizer::new();
        let mut messages: Vec<Message> = Vec::new();

        for step in steps {
            messages.extend(step);
            let conversation = Conversation::new_unvalidated(messages.clone());
            let (expected, expected_issues) = fix_conversation(conversation.clone());
            let (actual, actual_issues) = normalizer.normalize(conversation);

            assert_eq!(
                without_timestamps(actual.messages()),
                without_timestamps(expected.messages()),
                "incremental normalization diverged at {} messages",
                messages.len()
            );

            let mut expected_sorted = expected_issues.clone();
            let mut actual_sorted = actual_issues.clone();
            expected_sorted.sort();
            actual_sorted.sort();
            assert_eq!(
                actual_sorted,
                expected_sorted,
                "issue sets diverged at {} messages",
                messages.len()
            );
        }
    }

    #[test]
    fn incremental_matches_full_fix_for_plain_turns() {
        let steps = (0..30)
            .map(|i| vec![user(&format!("q{i}")), assistant(&format!("a{i}"))])
            .collect();
        assert_matches_full_fix(steps);
    }

    #[test]
    fn incremental_matches_full_fix_with_tool_pairs() {
        let mut steps = Vec::new();
        for i in 0..20 {
            let id = format!("call-{i}");
            steps.push(vec![
                user(&format!("do {i}")),
                tool_call(&id),
                tool_reply(&id, &format!("output {i}")),
                assistant(&format!("done {i}")),
            ]);
        }
        assert_matches_full_fix(steps);
    }

    #[test]
    fn incremental_matches_full_fix_when_a_turn_lands_one_message_at_a_time() {
        // The realistic shape: the agent appends the assistant tool-call message,
        // then the tool response, then the next assistant message — so a cut can
        // fall between a request and its response if the boundary is not clean.
        let mut steps = Vec::new();
        for i in 0..25 {
            let id = format!("call-{i}");
            steps.push(vec![user(&format!("q{i}"))]);
            steps.push(vec![tool_call(&id)]);
            steps.push(vec![tool_reply(&id, "ok")]);
            steps.push(vec![assistant(&format!("a{i}"))]);
        }
        assert_matches_full_fix(steps);
    }

    #[test]
    fn incremental_matches_full_fix_with_non_visible_messages() {
        let mut steps = Vec::new();
        for i in 0..20 {
            steps.push(vec![
                user(&format!("q{i}")),
                Message::user()
                    .with_text("hook context")
                    .with_visibility(true, false),
                assistant(&format!("a{i}  ")),
                Message::assistant()
                    .with_text("ui only")
                    .with_visibility(true, false),
            ]);
        }
        assert_matches_full_fix(steps);
    }

    #[test]
    fn incremental_matches_full_fix_with_mergeable_and_empty_messages() {
        let mut steps = Vec::new();
        for i in 0..20 {
            steps.push(vec![
                user(&format!("q{i}")),
                user("and another thing"),
                assistant(""),
                assistant(&format!("a{i}")),
            ]);
        }
        assert_matches_full_fix(steps);
    }

    #[test]
    fn incremental_matches_full_fix_with_orphaned_tool_content() {
        let mut steps = Vec::new();
        for i in 0..15 {
            let id = format!("call-{i}");
            steps.push(vec![
                user(&format!("q{i}")),
                tool_call(&id),
                // Never answered — a full fix strips the request.
                assistant(&format!("a{i}")),
                // Orphaned response with no request.
                tool_reply(&format!("ghost-{i}"), "nobody asked"),
            ]);
        }
        assert_matches_full_fix(steps);
    }

    #[test]
    fn rewriting_history_invalidates_the_cache() {
        let mut normalizer = ConversationNormalizer::new();
        let mut messages: Vec<Message> = Vec::new();
        for i in 0..15 {
            messages.push(user(&format!("q{i}")));
            messages.push(assistant(&format!("a{i}")));
        }
        let (_, _) = normalizer.normalize(Conversation::new_unvalidated(messages.clone()));

        // Compaction-style rewrite: hide the older prefix from the agent.
        for message in messages.iter_mut().take(6) {
            let metadata = message.metadata.with_agent_invisible();
            *message = message.clone().with_metadata(metadata);
        }
        messages.push(user("after compaction"));

        let conversation = Conversation::new_unvalidated(messages.clone());
        let (expected, _) = fix_conversation(conversation.clone());
        let (actual, _) = normalizer.normalize(conversation);
        assert_eq!(
            without_timestamps(actual.messages()),
            without_timestamps(expected.messages())
        );

        let (_hits, misses) = normalizer.stats();
        assert!(misses >= 2, "a rewritten prefix must miss the cache");
    }

    #[test]
    fn a_growing_last_message_does_not_poison_the_cache() {
        // Streaming appends text to the last assistant message in place.
        let mut normalizer = ConversationNormalizer::new();
        let mut messages: Vec<Message> = Vec::new();
        for i in 0..12 {
            messages.push(user(&format!("q{i}")));
            messages.push(assistant(&format!("a{i}")));
        }
        normalizer.normalize(Conversation::new_unvalidated(messages.clone()));

        for extra in ["chunk one", "chunk two", "chunk three"] {
            let last = messages.last_mut().unwrap();
            if let Some(MessageContent::Text(text)) = last.content.last_mut() {
                text.text.push_str(extra);
            }
            let conversation = Conversation::new_unvalidated(messages.clone());
            let (expected, _) = fix_conversation(conversation.clone());
            let (actual, _) = normalizer.normalize(conversation);
            assert_eq!(
                without_timestamps(actual.messages()),
                without_timestamps(expected.messages())
            );
        }
    }

    #[test]
    fn hits_the_cache_once_the_history_is_long_enough() {
        let mut normalizer = ConversationNormalizer::new();
        let mut messages: Vec<Message> = Vec::new();
        for i in 0..20 {
            messages.push(user(&format!("q{i}")));
            messages.push(assistant(&format!("a{i}")));
            normalizer.normalize(Conversation::new_unvalidated(messages.clone()));
        }
        let (hits, _misses) = normalizer.stats();
        assert!(
            hits > 10,
            "expected the frozen prefix to be reused, got {hits} hits"
        );
    }

    #[test]
    fn clean_cut_never_splits_a_tool_pair() {
        let messages = vec![
            user("q"),
            tool_call("a"),
            tool_reply("a", "out"),
            tool_call("b"),
            // "b" is still open here.
        ];
        assert_eq!(clean_cut(&messages, messages.len()), 3);
        assert_eq!(clean_cut(&messages, 2), 1);
    }

    #[test]
    fn fingerprint_changes_when_a_tool_result_is_rewritten() {
        let original = tool_reply("call-1", "a very long tool result");
        let pruned = tool_reply("call-1", "[truncated]");
        assert_ne!(
            message_fingerprint(&original),
            message_fingerprint(&pruned),
            "an in-place rewrite must invalidate the cached prefix"
        );
    }

    #[test]
    fn fingerprint_changes_with_visibility() {
        let visible = user("hello");
        let hidden = visible
            .clone()
            .with_metadata(visible.metadata.with_agent_invisible());
        assert_ne!(message_fingerprint(&visible), message_fingerprint(&hidden));
    }

    #[test]
    fn shared_normalizer_matches_full_fix() {
        let normalizer = SharedNormalizer::new();
        let mut messages: Vec<Message> = Vec::new();
        for i in 0..25 {
            messages.push(user(&format!("q{i}")));
            messages.push(assistant(&format!("a{i}")));
            let conversation = Conversation::new_unvalidated(messages.clone());
            let (expected, _) = fix_conversation(conversation.clone());
            let (actual, _) = normalizer.normalize(conversation);
            assert_eq!(
                without_timestamps(actual.messages()),
                without_timestamps(expected.messages())
            );
        }
    }

    /// Deterministic fuzz: build random conversations from the shapes the fix
    /// pipeline actually reacts to and assert incremental == full at every append.
    #[test]
    fn incremental_matches_full_fix_on_random_histories() {
        let mut seed = 0x5eed_1234_u64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for round in 0..40 {
            let mut normalizer = ConversationNormalizer::new();
            let mut messages: Vec<Message> = Vec::new();
            let mut open_call: Option<String> = None;

            for step in 0..40 {
                let choice = rand() % 8;
                let batch: Vec<Message> = match choice {
                    0 => vec![user(&format!("q{round}-{step}"))],
                    1 => vec![assistant(&format!("a{round}-{step}  "))],
                    2 => {
                        let id = format!("call-{round}-{step}");
                        open_call = Some(id.clone());
                        vec![tool_call(&id)]
                    }
                    3 => match open_call.take() {
                        Some(id) => vec![tool_reply(&id, "output")],
                        None => vec![tool_reply(&format!("ghost-{step}"), "orphan")],
                    },
                    4 => vec![Message::user()
                        .with_text("invisible")
                        .with_visibility(true, false)],
                    5 => vec![assistant("")],
                    6 => vec![
                        user(&format!("q{step}")),
                        user("double user"),
                        assistant(&format!("a{step}")),
                    ],
                    _ => {
                        let id = format!("call-{round}-{step}");
                        vec![
                            tool_call(&id),
                            tool_reply(&id, "output"),
                            assistant("summary"),
                        ]
                    }
                };
                messages.extend(batch);

                let conversation = Conversation::new_unvalidated(messages.clone());
                let (expected, expected_issues) = fix_conversation(conversation.clone());
                let (actual, actual_issues) = normalizer.normalize(conversation);
                assert_eq!(
                    without_timestamps(actual.messages()),
                    without_timestamps(expected.messages()),
                    "round {round} step {step}: incremental diverged from the full fix"
                );

                let mut expected_sorted = expected_issues;
                let mut actual_sorted = actual_issues;
                expected_sorted.sort();
                actual_sorted.sort();
                assert_eq!(
                    actual_sorted, expected_sorted,
                    "round {round} step {step}: issue sets diverged"
                );
            }
        }
    }

    #[test]
    fn tool_request_ids_survive_a_frozen_prefix() {
        // A tool request in the frozen prefix answered in the suffix would be
        // stripped as orphaned if the cut were not clean.
        let mut normalizer = ConversationNormalizer::new();
        let mut messages: Vec<Message> = Vec::new();
        for i in 0..12 {
            messages.push(user(&format!("q{i}")));
            messages.push(assistant(&format!("a{i}")));
        }
        normalizer.normalize(Conversation::new_unvalidated(messages.clone()));

        messages.push(tool_call("late"));
        normalizer.normalize(Conversation::new_unvalidated(messages.clone()));

        messages.push(tool_reply("late", "landed"));
        let conversation = Conversation::new_unvalidated(messages.clone());
        let (expected, _) = fix_conversation(conversation.clone());
        let (actual, _) = normalizer.normalize(conversation);
        assert_eq!(
            without_timestamps(actual.messages()),
            without_timestamps(expected.messages())
        );
        assert!(actual.messages().iter().any(|m| m
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(req) if req.id == "late"))));
    }
}
