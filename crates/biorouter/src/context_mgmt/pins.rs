//! #51 part (b): the preservation marker's selection and budget.
//!
//! [`MessageMetadata::pinned`] means "carry this message verbatim through every
//! compaction; never dissolve it into a summary". This module decides, for one
//! compaction, WHICH marked messages that promise is actually kept for, and
//! makes it loud when it is not.
//!
//! The operator's retention rule, stated once and implemented here:
//!
//! * A preserved message survives **summarization** unconditionally. No
//!   summarizer ever sees it and no compaction path ever flips it
//!   agent-invisible.
//! * It is still subject to the overall **context budget**. A session cannot
//!   accumulate pins forever, because a conversation whose pinned set fills the
//!   window can no longer be compacted at all — it would deadlock on the very
//!   overflow the compactor exists to resolve.
//! * When the budget bites, the **oldest** pins lose their exemption first, and
//!   that is reported rather than silent.
//!
//! So the guarantee is "never dropped by summarization", not "never dropped".
//! Losing the exemption is a demotion, not a deletion: the message stays in the
//! transcript and is summarized like any other older message.
//!
//! [`MessageMetadata::pinned`]: crate::conversation::message::MessageMetadata::pinned

use crate::config::Config;
use crate::conversation::message::SystemNotificationType;
use crate::conversation::message::{Message, MessageContent, MessageMetadata};
use tracing::warn;

/// #51: how many messages may hold the preservation marker and still be
/// honoured in one compaction. Overridable with `BIOROUTER_MAX_PINNED_MESSAGES`;
/// `0` disables pinning entirely (every pin is reported as evicted, so a
/// mis-set value cannot be mistaken for "there were no pins").
///
/// 32 is deliberately generous for the intended use — standing instructions and
/// notes handed to a running session — while being far too small to let a
/// runaway producer wedge a conversation.
pub const DEFAULT_MAX_PINNED_MESSAGES: usize = 32;

/// #51: the share of the model's context window the honoured pinned set may
/// occupy. Overridable with `BIOROUTER_MAX_PINNED_CONTEXT_SHARE`.
///
/// The count cap alone is not a bound: 32 pinned messages can be 32 pasted
/// files. This is the cap that actually keeps a conversation compactable, since
/// it scales with the window rather than with a message count. A quarter of the
/// window leaves three quarters for the summary plus the verbatim recent
/// turns — the material the model needs to act at all.
pub const DEFAULT_MAX_PINNED_CONTEXT_SHARE: f64 = 0.25;

/// The bound on one compaction's honoured pinned set. Both caps apply; the
/// first one reached ends the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinLimits {
    pub max_messages: usize,
    pub max_tokens: usize,
}

impl PinLimits {
    /// Read the configured caps, resolving the token share against the model's
    /// context window. Same style as `BIOROUTER_COMPACT_KEEP_LAST_TURNS`.
    pub fn from_config(context_limit: usize) -> Self {
        let config = Config::global();
        let max_messages = config
            .get_param::<usize>("BIOROUTER_MAX_PINNED_MESSAGES")
            .unwrap_or(DEFAULT_MAX_PINNED_MESSAGES);
        let share = config
            .get_param::<f64>("BIOROUTER_MAX_PINNED_CONTEXT_SHARE")
            .unwrap_or(DEFAULT_MAX_PINNED_CONTEXT_SHARE)
            .clamp(0.0, 1.0);
        Self {
            max_messages,
            max_tokens: (context_limit as f64 * share) as usize,
        }
    }
}

/// Why one marked message lost its exemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinEvictionReason {
    /// More marked messages than [`PinLimits::max_messages`].
    CountCap,
    /// The honoured set would have exceeded [`PinLimits::max_tokens`].
    TokenBudget,
}

impl PinEvictionReason {
    fn describe(self) -> &'static str {
        match self {
            PinEvictionReason::CountCap => "the preserved-message count cap",
            PinEvictionReason::TokenBudget => "the preserved-set token budget",
        }
    }
}

/// One marked message that will be summarized like any other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictedPin {
    /// Index into the conversation the selection was computed over.
    pub index: usize,
    /// The message's durable id, when it has one.
    pub id: Option<String>,
    pub reason: PinEvictionReason,
}

/// Which marked messages this compaction will honour.
///
/// Indexed positionally against the exact `&[Message]` it was computed from —
/// build it once at the top of a compaction and thread it through, never
/// recompute it against a different slice.
#[derive(Debug, Clone, Default)]
pub struct PinSelection {
    honoured: Vec<bool>,
    /// Oldest-first, which is also eviction order.
    evicted: Vec<EvictedPin>,
    honoured_count: usize,
    honoured_tokens: usize,
    limits: Option<PinLimits>,
}

impl PinSelection {
    /// A selection over `len` messages that honours nothing — for callers with
    /// no provider context to budget against.
    pub fn none(len: usize) -> Self {
        Self {
            honoured: vec![false; len],
            ..Default::default()
        }
    }

    pub fn is_honoured(&self, index: usize) -> bool {
        self.honoured.get(index).copied().unwrap_or(false)
    }

    pub fn honoured_count(&self) -> usize {
        self.honoured_count
    }

    pub fn honoured_tokens(&self) -> usize {
        self.honoured_tokens
    }

    pub fn evicted(&self) -> &[EvictedPin] {
        &self.evicted
    }

    /// Whether this compaction had anything to decide at all.
    pub fn is_empty(&self) -> bool {
        self.honoured_count == 0 && self.evicted.is_empty()
    }

    /// Log the eviction. Called on EVERY compaction path, so no path can drop a
    /// preserved message quietly — the in-conversation notices below are the
    /// user- and model-facing half of the same event.
    pub fn log(&self) {
        if self.evicted.is_empty() {
            return;
        }
        let limits = self.limits.unwrap_or(PinLimits {
            max_messages: 0,
            max_tokens: 0,
        });
        let ids: Vec<&str> = self
            .evicted
            .iter()
            .filter_map(|e| e.id.as_deref())
            .collect();
        warn!(
            evicted = self.evicted.len(),
            honoured = self.honoured_count,
            honoured_tokens = self.honoured_tokens,
            max_messages = limits.max_messages,
            max_tokens = limits.max_tokens,
            evicted_ids = ?ids,
            "#51: the preserved set exceeded its budget; the OLDEST preserved \
             messages lost their summarization exemption and are now summarized \
             like any other older message"
        );
    }

    /// The messages to splice into the compacted conversation when the budget
    /// bit, so the eviction is visible in the transcript and not only in a log
    /// nobody reads.
    ///
    /// Two messages, because the two audiences need different things and a
    /// `SystemNotification` must never reach a provider (the Bedrock formatter
    /// hard-errors on one):
    ///
    /// 1. an agent-only text note, so the model knows a message it was told to
    ///    keep has lost that protection;
    /// 2. a user-only inline notification, so the human who pinned it finds out.
    pub fn eviction_notices(&self) -> Vec<Message> {
        if self.evicted.is_empty() {
            return Vec::new();
        }
        let n = self.evicted.len();
        let plural = if n == 1 { "message" } else { "messages" };
        let reason = self
            .evicted
            .first()
            .map(|e| e.reason.describe())
            .unwrap_or("the preserved-set budget");
        let limits = self.limits.unwrap_or(PinLimits {
            max_messages: 0,
            max_tokens: 0,
        });

        // The wording is deliberately "no longer exempt" rather than "has been
        // condensed". Selection is newest-first, so an evicted pin is always
        // older than every honoured one — but when the budget cannot hold even
        // the newest, an evicted pin can still be sitting inside the verbatim
        // recent window this time round. It has genuinely lost its protection
        // (the next compaction will summarize it), which is what to say; that
        // it was condensed *already* would not always be true.
        vec![
            Message::assistant()
                .with_text(format!(
                    "{n} earlier {plural} marked to be preserved verbatim could not all be \
                     kept: the marked set exceeded {reason}. The oldest of them are no \
                     longer exempt from summarization, so rely on the summary rather than \
                     on their exact wording.",
                ))
                .with_metadata(MessageMetadata::agent_only()),
            Message::assistant().with_system_notification(
                SystemNotificationType::InlineMessage,
                format!(
                    "Compaction could not preserve {n} pinned {plural}: the pinned set \
                     exceeded {reason} (at most {} messages and ~{} tokens). The oldest \
                     pinned {plural} are no longer protected from summarization.",
                    limits.max_messages, limits.max_tokens,
                ),
            ),
        ]
    }
}

/// Whether a marked message's pin can be honoured at all.
///
/// Pinning is for STANDALONE messages — exactly the shape of the first consumer
/// (BR-71's `mode: "note"`). The per-variant verdict, and the reasoning behind
/// each one, lives in [`MessageContent::is_pin_eligible`] as an exhaustive
/// match, so a new content variant cannot default to "preservable" the way
/// `FrontendToolRequest` once did. EVERY block of the message must be eligible:
/// a pin is honoured on the whole message, so one tool-protocol block in it is
/// enough to make preserving the message unsafe.
///
/// An already agent-invisible message is also not eligible: a pin means "do not
/// summarize this away", not "resurrect this". Without that clause a pin would
/// undo `/compact`'s own hiding on the next pass.
pub fn pin_is_eligible(message: &Message) -> bool {
    message.metadata.pinned
        && message.is_agent_visible()
        && message.content.iter().all(MessageContent::is_pin_eligible)
}

/// Decide which of `messages`' pins this compaction honours.
///
/// Walks the eligible pins NEWEST-FIRST and stops at the first one that does
/// not fit; everything older than that is evicted too. Stopping (rather than
/// continuing to look for a smaller older pin that would still fit) is what
/// makes the kept set a contiguous newest suffix, which is the operator's rule:
/// the oldest preserved messages are the first to be dropped.
pub fn select_pins(messages: &[Message], limits: PinLimits) -> PinSelection {
    let eligible: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| pin_is_eligible(message))
        .map(|(index, _)| index)
        .collect();

    let mut selection = PinSelection {
        honoured: vec![false; messages.len()],
        evicted: Vec::new(),
        honoured_count: 0,
        honoured_tokens: 0,
        limits: Some(limits),
    };
    if eligible.is_empty() {
        return selection;
    }

    let mut exhausted: Option<PinEvictionReason> = None;
    for &index in eligible.iter().rev() {
        let evict = |reason: PinEvictionReason, selection: &mut PinSelection| {
            selection.evicted.push(EvictedPin {
                index,
                id: messages[index].id.clone(),
                reason,
            });
        };

        if let Some(reason) = exhausted {
            evict(reason, &mut selection);
            continue;
        }
        if selection.honoured_count >= limits.max_messages {
            exhausted = Some(PinEvictionReason::CountCap);
            evict(PinEvictionReason::CountCap, &mut selection);
            continue;
        }
        let cost = super::estimate_message_tokens(&messages[index]);
        if selection.honoured_tokens + cost > limits.max_tokens {
            exhausted = Some(PinEvictionReason::TokenBudget);
            evict(PinEvictionReason::TokenBudget, &mut selection);
            continue;
        }

        selection.honoured[index] = true;
        selection.honoured_count += 1;
        selection.honoured_tokens += cost;
    }

    // Collected newest-first; report and reason about them oldest-first.
    selection.evicted.reverse();
    selection
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned(text: &str) -> Message {
        Message::user().with_text(text).pinned()
    }

    fn generous() -> PinLimits {
        PinLimits {
            max_messages: 32,
            max_tokens: 100_000,
        }
    }

    #[test]
    fn an_unmarked_conversation_selects_nothing_and_reports_nothing() {
        let messages = vec![
            Message::user().with_text("q"),
            Message::assistant().with_text("a"),
        ];
        let selection = select_pins(&messages, generous());
        assert!(selection.is_empty());
        assert_eq!(selection.honoured_count(), 0);
        assert!(selection.evicted().is_empty());
        assert!(selection.eviction_notices().is_empty());
    }

    #[test]
    fn every_eligible_pin_is_honoured_under_a_generous_budget() {
        let messages = vec![
            pinned("one"),
            Message::assistant().with_text("a"),
            pinned("two"),
        ];
        let selection = select_pins(&messages, generous());
        assert!(selection.is_honoured(0));
        assert!(!selection.is_honoured(1));
        assert!(selection.is_honoured(2));
        assert_eq!(selection.honoured_count(), 2);
        assert!(selection.evicted().is_empty());
    }

    /// The count cap evicts OLDEST-first, and the kept set is the newest
    /// contiguous suffix.
    #[test]
    fn the_count_cap_evicts_the_oldest_pins_first() {
        let messages: Vec<Message> = (0..5).map(|i| pinned(&format!("note-{i}"))).collect();
        let selection = select_pins(
            &messages,
            PinLimits {
                max_messages: 2,
                max_tokens: 100_000,
            },
        );

        assert_eq!(selection.honoured_count(), 2);
        assert!(selection.is_honoured(3) && selection.is_honoured(4));
        assert_eq!(
            selection
                .evicted()
                .iter()
                .map(|e| e.index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2],
            "evictions are reported oldest-first"
        );
        assert!(selection
            .evicted()
            .iter()
            .all(|e| e.reason == PinEvictionReason::CountCap));
    }

    /// The token budget is the cap that actually keeps a conversation
    /// compactable, because it scales with the window rather than a count.
    #[test]
    fn the_token_budget_evicts_the_oldest_pins_first() {
        let messages: Vec<Message> = (0..4)
            .map(|i| pinned(&format!("{} {}", i, "x".repeat(4_000))))
            .collect();
        let per_message = super::super::estimate_message_tokens(&messages[0]);
        // Room for exactly two.
        let selection = select_pins(
            &messages,
            PinLimits {
                max_messages: 32,
                max_tokens: per_message * 2 + 1,
            },
        );

        assert_eq!(selection.honoured_count(), 2);
        assert!(selection.is_honoured(2) && selection.is_honoured(3));
        assert_eq!(
            selection
                .evicted()
                .iter()
                .map(|e| e.index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!(selection
            .evicted()
            .iter()
            .all(|e| e.reason == PinEvictionReason::TokenBudget));
    }

    /// A single pin larger than the whole budget must be evicted, not
    /// deadlock the selection or be admitted "because it is the newest".
    #[test]
    fn a_pin_larger_than_the_entire_budget_is_evicted() {
        let messages = vec![pinned(&"x".repeat(200_000))];
        let selection = select_pins(
            &messages,
            PinLimits {
                max_messages: 32,
                max_tokens: 100,
            },
        );
        assert_eq!(selection.honoured_count(), 0);
        assert_eq!(selection.evicted().len(), 1);
        assert!(!selection.eviction_notices().is_empty());
    }

    /// `max_messages: 0` is the "pinning off" switch. It must be reported, not
    /// silently indistinguishable from a conversation with no pins.
    #[test]
    fn a_zero_cap_disables_pinning_loudly() {
        let messages = vec![pinned("note")];
        let selection = select_pins(
            &messages,
            PinLimits {
                max_messages: 0,
                max_tokens: 100_000,
            },
        );
        assert_eq!(selection.honoured_count(), 0);
        assert_eq!(selection.evicted().len(), 1);
        assert!(!selection.is_empty(), "a disabled pin is still an event");
    }

    /// Eligibility: tool halves and already-hidden messages are never honoured.
    #[test]
    fn ineligible_pins_are_not_selected_and_are_not_reported_as_evicted() {
        let tool = Message::assistant()
            .with_tool_request(
                "tool_0",
                Ok(rmcp::model::CallToolRequestParams {
                    task: None,
                    name: "read_file".into(),
                    arguments: None,
                    meta: None,
                }),
            )
            .pinned();
        let hidden = Message::user()
            .with_text("archived")
            .pinned()
            .with_visibility(true, false);
        let messages = vec![tool, hidden, pinned("real")];

        let selection = select_pins(&messages, generous());
        assert!(!selection.is_honoured(0));
        assert!(!selection.is_honoured(1));
        assert!(selection.is_honoured(2));
        assert!(
            selection.evicted().is_empty(),
            "ineligible is not the same as budget-evicted"
        );
    }

    /// The two notices target different audiences, and the one carrying a
    /// `SystemNotification` must be agent-INVISIBLE: the Bedrock formatter
    /// hard-errors if one ever reaches a provider.
    #[test]
    fn eviction_notices_are_split_by_audience() {
        let messages: Vec<Message> = (0..3).map(|i| pinned(&format!("n{i}"))).collect();
        let selection = select_pins(
            &messages,
            PinLimits {
                max_messages: 1,
                max_tokens: 100_000,
            },
        );

        let notices = selection.eviction_notices();
        assert_eq!(notices.len(), 2);

        let agent = &notices[0];
        assert!(agent.is_agent_visible() && !agent.is_user_visible());
        assert!(agent
            .content
            .iter()
            .all(|c| matches!(c, MessageContent::Text(_))));

        let user = &notices[1];
        assert!(user.is_user_visible() && !user.is_agent_visible());
        assert!(user
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::SystemNotification(_))));
        assert!(!user.is_pinned(), "a notice must not itself be pinned");
    }

    fn call(
        name: &str,
        arguments: Option<rmcp::model::JsonObject>,
    ) -> crate::mcp_utils::ToolResult<rmcp::model::CallToolRequestParams> {
        Ok(rmcp::model::CallToolRequestParams {
            task: None,
            name: name.to_string().into(),
            arguments,
            meta: None,
        })
    }

    /// #51 / W8: a `FrontendToolRequest` is a provider tool-protocol block —
    /// every formatter turns it into a real `tool_use` (bedrock.rs, anthropic.rs,
    /// openai.rs, databricks.rs). Honouring a pin on one preserves half of a
    /// tool pair through a compaction that summarizes the other half away, which
    /// is the exact hazard the tool-content exclusion exists to prevent.
    ///
    /// It reaches here on a USER-role message because the normalizer only strips
    /// `FrontendToolRequest` from ASSISTANT messages (`conversation/mod.rs`), so
    /// the unvalidated API is a real path to one.
    #[test]
    fn a_pinned_frontend_tool_request_is_never_honoured() {
        let messages = vec![
            Message::user()
                .with_frontend_tool_request("frontend_0", call("open_file", None))
                .pinned(),
            pinned("a real note"),
        ];

        let selection = select_pins(&messages, generous());
        assert!(
            !selection.is_honoured(0),
            "a frontend tool request must not be exempt from summarization"
        );
        assert!(selection.is_honoured(1));
        assert!(
            selection.evicted().is_empty(),
            "ineligible is not the same as budget-evicted"
        );
    }

    /// #51 / W8: the pinned-set BUDGET must see a `FrontendToolRequest`'s
    /// arguments. They are model-visible (they become the `tool_use` block's
    /// `input`), so a pin carrying a megabyte of them that costs ~0 tokens is an
    /// unbounded evasion of both the pin budget and the compaction trigger — the
    /// same estimate feeds both.
    #[test]
    fn the_byte_estimate_counts_frontend_tool_request_arguments() {
        let mut arguments = rmcp::model::JsonObject::new();
        arguments.insert("body".to_string(), serde_json::json!("x".repeat(40_000)));
        let message = Message::user()
            .with_frontend_tool_request("frontend_0", call("write", Some(arguments)));

        let equivalent_request = Message::assistant().with_tool_request(
            "frontend_0",
            call("write", {
                let mut a = rmcp::model::JsonObject::new();
                a.insert("body".to_string(), serde_json::json!("x".repeat(40_000)));
                Some(a)
            }),
        );

        assert_eq!(
            super::super::estimate_message_tokens(&message),
            super::super::estimate_message_tokens(&equivalent_request),
            "a frontend tool request costs the same as the tool request it becomes"
        );
        assert!(
            super::super::estimate_message_tokens(&message) > 10_000,
            "40k bytes of arguments must not estimate as an empty message"
        );
    }

    /// #51 / W8: the eligibility rule is a verdict on EVERY [`MessageContent`]
    /// variant, not on the three that happened to be named when it was written.
    /// A new variant defaults to *eligible* under a `matches!` exclusion list,
    /// which is the failure mode that let `FrontendToolRequest` through — so the
    /// rule is an exhaustive match and this is its table.
    #[test]
    fn every_message_content_variant_has_an_explicit_pin_eligibility_verdict() {
        let mut arguments = rmcp::model::JsonObject::new();
        arguments.insert("path".to_string(), serde_json::json!("/tmp/x"));

        // (name, message, eligible?)
        let cases: Vec<(&str, Message, bool)> = vec![
            // Self-contained: carries its whole meaning alone, and no provider
            // requires a partner block elsewhere in the transcript.
            ("Text", Message::user().with_text("note"), true),
            (
                "Image",
                Message::user().with_image("ZGF0YQ==", "image/png"),
                true,
            ),
            // Provider tool-protocol halves: preserving one past a compaction
            // that hides the other hands the provider a dangling tool call.
            (
                "ToolRequest",
                Message::assistant().with_tool_request("t0", call("read_file", None)),
                false,
            ),
            (
                "ToolResponse",
                Message::user()
                    .with_tool_response("t0", Ok(rmcp::model::CallToolResult::success(vec![]))),
                false,
            ),
            (
                "FrontendToolRequest",
                Message::user().with_frontend_tool_request("f0", call("open_file", None)),
                false,
            ),
            // UI handshakes keyed to a pending request. Every formatter drops
            // them or emits an empty block, so a pin spends budget on nothing
            // and outlives the request it refers to.
            (
                "ToolConfirmationRequest",
                Message::user().with_content(MessageContent::ToolConfirmationRequest(
                    crate::conversation::message::ToolConfirmationRequest {
                        id: "c0".to_string(),
                        tool_name: "shell".to_string(),
                        arguments: arguments.clone(),
                        prompt: None,
                    },
                )),
                false,
            ),
            (
                "ActionRequired",
                Message::user().with_action_required(
                    "a0",
                    "shell".to_string(),
                    arguments.clone(),
                    None,
                ),
                false,
            ),
            // Bound to the assistant turn that produced them: the signature is
            // validated against that turn, and Bedrock drops them entirely.
            (
                "Thinking",
                Message::assistant().with_thinking("reasoning", "sig"),
                false,
            ),
            (
                "RedactedThinking",
                Message::assistant().with_redacted_thinking("blob"),
                false,
            ),
            // User-facing only; the Bedrock formatter hard-errors on one.
            (
                "SystemNotification",
                Message::assistant().with_content(MessageContent::system_notification(
                    SystemNotificationType::InlineMessage,
                    "hi",
                )),
                false,
            ),
        ];

        assert_eq!(
            cases.len(),
            10,
            "every MessageContent variant needs a row; add one when a variant is added"
        );

        for (name, message, eligible) in cases {
            let message = message.pinned();
            assert!(
                message.is_agent_visible(),
                "{name}: the fixture must isolate content eligibility from visibility"
            );
            assert_eq!(
                pin_is_eligible(&message),
                eligible,
                "{name}: wrong pin-eligibility verdict"
            );
        }
    }

    /// The share cap resolves against the model's own window, and a nonsense
    /// share is clamped rather than producing a negative or absurd budget.
    #[test]
    fn limits_resolve_the_share_against_the_context_window() {
        let limits = PinLimits::from_config(200_000);
        assert_eq!(limits.max_messages, DEFAULT_MAX_PINNED_MESSAGES);
        assert_eq!(
            limits.max_tokens,
            (200_000.0 * DEFAULT_MAX_PINNED_CONTEXT_SHARE) as usize
        );
        // A tiny window still yields a usable (if small) non-panicking budget.
        assert_eq!(PinLimits::from_config(0).max_tokens, 0);
    }
}
