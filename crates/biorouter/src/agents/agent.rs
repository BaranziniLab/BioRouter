use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use uuid::Uuid;

use super::final_output_tool::FinalOutputTool;
use super::platform_tools;
use super::tool_execution::{ToolCallResult, CHAT_MODE_TOOL_SKIPPED_RESPONSE, DECLINED_RESPONSE};
use super::turn_abort::TurnAbortCode;
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::budget::{BudgetAction, BudgetTracker, ReplyBudget};
use crate::agents::effort::ReasoningEffort;
use crate::agents::extension::{ExtensionConfig, ExtensionResult, ToolInfo};
use crate::agents::extension_manager::{
    get_parameter_names, normalize, resolve_bundled_extension, ExtensionManager,
};
use crate::agents::extension_manager_extension::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::final_output_tool::{FINAL_OUTPUT_CONTINUATION_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
use crate::agents::platform_tools::{
    PLATFORM_INGEST_CONVERSATION_TOOL_NAME, PLATFORM_MANAGE_SCHEDULE_TOOL_NAME,
    PLATFORM_READ_SESSION_BLOB_TOOL_NAME,
};
use crate::agents::prompt_manager::PromptManager;
use crate::agents::resource_refs::{extract_resource_refs, ResourceRefs};
use crate::agents::retry::{RetryManager, RetryResult};
use crate::agents::stall::{StallAction, StallCheckConfig, StallWatch};
use crate::agents::subagent_task_config::TaskConfig;
use crate::agents::subagent_tool::{handle_subagent_tool, SUBAGENT_TOOL_NAME};
use crate::agents::types::SessionConfig;
use crate::agents::types::{FrontendTool, SharedProvider, ToolResultReceiver};
use crate::checkpoint::{CheckpointConfig, CheckpointKind, CheckpointManager};
use crate::config::permission::PermissionManager;
use crate::config::{BioRouterMode, Config};
use crate::context_mgmt::{
    check_if_compaction_needed, compact_messages, compact_messages_with_recovery,
    overflow_recovery_for_attempt, DEFAULT_COMPACTION_THRESHOLD,
};
use crate::conversation::message::{
    new_message_id, ActionRequiredData, Message, MessageContent, ProvenanceKind, ProviderMetadata,
    SystemNotificationType, TokenState, ToolRequest,
};
use crate::conversation::tool_result_serde::call_tool_result;
use crate::conversation::{
    debug_conversation_fix, fix_conversation, has_signed_reasoning, Conversation,
};
use crate::managed::ManagedPolicy;
use crate::mcp_utils::ToolResult;
use crate::observability::loop_safety::{self, LoopSafetyEvent, LoopSafetyKind};
use crate::permission::managed_inspector::ManagedPolicyInspector;
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::permission::tool_risk::ToolRiskRegistry;
use crate::permission::PermissionConfirmation;
use crate::privacy::refusal::PrivacyRefusal;
use crate::privacy::{ProviderTier, SessionClassification};
use crate::providers::base::Provider;
use crate::providers::coding_agent::bridge as coding_agent_bridge;
use crate::providers::errors::ProviderError;
use crate::scheduler_trait::SchedulerTrait;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::message_blobs;
use crate::session::session_manager::BindOutcome;
use crate::session::{Session, SessionManager, SessionType};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspectionManager};
use crate::tool_monitor::{FailureLoopConfig, RepetitionInspector, SemanticLoopConfig};
use crate::utils::is_token_cancelled;
use crate::workflow::{Author, Response, Settings, SubWorkflow, Workflow};
use regex::Regex;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData, GetPromptResult, Prompt,
    ServerNotification, Tool,
};
use rmcp::object;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

const DEFAULT_MAX_TURNS: u32 = 100;
/// Absolute cap on the number of tool calls in a single reply, summed across all
/// iterations. `max_turns` counts provider round-trips, but one round-trip can
/// fan out many parallel tool calls, so a few iterations can run an unbounded
/// number of tools with ever-changing args (which the exact-duplicate guard
/// misses). This is the backstop for that. Generous by default so it never bites
/// normal work; overridable per session (`max_tool_calls`) or globally
/// (`BIOROUTER_MAX_TOOL_CALLS`).
const DEFAULT_MAX_TOOL_CALLS: u32 = 200;
/// BR-29 staged repetition guard, soft stage: the Nth consecutive byte-identical
/// tool call earns a non-blocking warning injected into the model's context (the
/// call still runs). Overridable with `BIOROUTER_REPETITION_SOFT_WARN`.
const DEFAULT_REPETITION_SOFT_WARN: u32 = 3;
/// BR-29 staged repetition guard, hard stage: the Nth consecutive byte-identical
/// tool call is denied outright, with an honest "repetition guard" reason (never
/// the misleading "the user declined"). Overridable with
/// `BIOROUTER_REPETITION_HARD_STOP`. Set below the soft threshold to disable the
/// soft stage entirely.
const DEFAULT_REPETITION_HARD_STOP: u32 = 5;
const COMPACTION_THINKING_TEXT: &str = "biorouter is compacting the chat...";
/// Total auto-continues available to one user reply when the provider ends a
/// response with `finish_reason == "length"`. This never resets inside the
/// reply: a tool call, malformed call, or provider retry cannot replenish it.
const MAX_TRUNCATION_CONTINUATIONS: u32 = 12;
/// A reasoning-only or otherwise empty truncation has a tighter budget. These
/// are counted cumulatively across the reply as well, so alternating empty
/// output with tools cannot turn the general cap into another continuation
/// storm.
const MAX_ZERO_PROGRESS_TRUNCATION_CONTINUATIONS: u32 = 3;
/// Injected when auto-continuing a length-truncated turn, so the model resumes
/// instead of the agent ending the turn on a half-finished response.
const TRUNCATION_CONTINUATION_MESSAGE: &str = "Your previous response was cut off because it reached the output length limit (finish_reason=\"length\"). Continue exactly where you left off, and do not repeat what you already wrote.";
fn canonicalize_signed_replay_suffix(messages: &[Message]) -> Conversation {
    let mut grouped = Vec::<Message>::new();
    for message in messages.iter().cloned() {
        if message.role == rmcp::model::Role::User {
            if let Some(previous) = grouped
                .last_mut()
                .filter(|previous| previous.role == rmcp::model::Role::User)
            {
                previous.content.extend(message.content);
                continue;
            }
        }
        grouped.push(message);
    }
    Conversation::new_unvalidated(grouped)
}

fn continues_bedrock_assistant_construction(messages: &[Message]) -> bool {
    let continues_with_tool_results = messages.iter().any(|message| {
        message
            .content
            .iter()
            .any(|content| matches!(content, MessageContent::ToolResponse(_)))
    });
    let continues_after_hidden_length = messages.iter().any(|message| {
        message.role == rmcp::model::Role::User
            && !message.is_user_visible()
            && message.is_agent_visible()
            && message.as_concat_text() == TRUNCATION_CONTINUATION_MESSAGE
    });
    continues_with_tool_results || continues_after_hidden_length
}

#[derive(Default)]
struct TruncationRecoveryBudget {
    continuations: u32,
    zero_progress_continuations: u32,
}

enum TruncationRecoveryAction {
    Continue,
    Exhausted { zero_progress: bool },
}

impl TruncationRecoveryBudget {
    fn observe(&mut self, made_user_visible_progress: bool) -> TruncationRecoveryAction {
        if self.continuations >= MAX_TRUNCATION_CONTINUATIONS {
            return TruncationRecoveryAction::Exhausted {
                zero_progress: false,
            };
        }
        if !made_user_visible_progress
            && self.zero_progress_continuations >= MAX_ZERO_PROGRESS_TRUNCATION_CONTINUATIONS
        {
            return TruncationRecoveryAction::Exhausted {
                zero_progress: true,
            };
        }

        self.continuations += 1;
        if !made_user_visible_progress {
            self.zero_progress_continuations += 1;
        }
        TruncationRecoveryAction::Continue
    }
}

/// Whether this response continues a provider-**signed** assistant turn — it
/// either carries the signature itself, or an earlier chunk stored under the
/// same id does.
///
/// Only such a turn may be folded back into one row. A reasoning signature
/// authenticates the exact block list the provider emitted, so that grouping has
/// to be reconstructed before anything is appended after it. Every other
/// provider must keep `Conversation::push` semantics, which merge only an
/// *adjacent* same-id row: folding across an intervening tool-result row would
/// persist the model's post-tool prose *before* the result it describes.
fn continues_signed_turn(response: &Message, pending: &Conversation) -> bool {
    has_signed_reasoning(response)
        || response.id.as_ref().is_some_and(|response_id| {
            pending.messages().iter().any(|message| {
                message.id.as_ref() == Some(response_id) && has_signed_reasoning(message)
            })
        })
}

fn message_has_user_visible_progress(message: &Message) -> bool {
    message.content.iter().any(|content| match content {
        MessageContent::Text(text) => !text.text.trim().is_empty(),
        MessageContent::Image(_) => true,
        _ => false,
    })
}

/// The message id to stamp on the assistant-side messages the loop rebuilds for
/// a reply that requested tools (the preserved thinking block and the tool
/// requests themselves).
///
/// This must be the *provider's* id for the reply whenever there is one.
/// `Conversation::push` merges a pushed message into the previous one only when
/// their ids match, and on a streaming provider the thinking block and the
/// tool_use block arrive as two separate chunks that share the provider's
/// `message_id`: the thinking-only chunk is pushed verbatim (it requests no
/// tools), and the tool-bearing chunk is rebuilt here. Stamping a fresh uuid on
/// the rebuilt message leaves the two unmerged, so the request body carries two
/// consecutive `assistant` entries and the tool-bearing one opens with
/// `tool_use`. Anthropic rejects that outright when extended thinking is on —
/// the final assistant message must begin with a thinking block.
///
/// Falls back to a fresh uuid only when the provider supplied no id, which is
/// exactly the pre-existing behaviour for those providers.
///
/// The caller must use this id for at most ONE message per reply — the session
/// store enforces `UNIQUE (session_id, msg_uid)`, so a second message carrying
/// the same id fails the whole turn at persist time. Only the first rebuilt
/// message can merge anyway: consecutive tool requests are separated by their
/// tool-response (user) messages, which breaks the `Conversation::push`
/// last-message match.
pub(crate) fn assistant_turn_message_id(response: &Message) -> String {
    response
        .id
        .clone()
        .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4()))
}

/// Defense-in-depth for issue #41: one persist batch must never contain two
/// messages with the same id — the session store enforces
/// `UNIQUE(session_id, msg_uid)` and a single collision aborts the whole turn
/// with SQLite error 2067.
///
/// Adjacent same-id messages were already merged by [`Conversation::push`], so
/// any duplicate id left in the batch is **non-adjacent**: a decoder that
/// stamped one shared per-response id on several yielded messages (the Bedrock
/// decoder before its §6.2b batching; any streaming decoder with batching
/// disabled) whose rebuilt tool requests ended up separated by their tool
/// responses. Re-mint a fresh id on every later occurrence so the turn
/// persists instead of dying; the first occurrence keeps the provider's id so
/// desktop delta-merging is unaffected. Messages without ids are left alone —
/// the store mints a fresh uid for each of those on insert.
pub(crate) fn remint_duplicate_message_ids(batch: Conversation) -> Conversation {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut messages = batch.into_messages();
    for message in &mut messages {
        let Some(id) = message.id.clone() else {
            continue;
        };
        if !seen.insert(id.clone()) {
            let fresh = format!("msg_{}", Uuid::new_v4());
            warn!(
                old_id = %id,
                new_id = %fresh,
                "duplicate message id within one persist batch; re-minting to avoid a \
                 UNIQUE(session_id, msg_uid) abort (decoder id-reuse bug?)"
            );
            seen.insert(fresh.clone());
            message.id = Some(fresh);
        }
    }
    Conversation::new_unvalidated(messages)
}

/// #59: name a message the loop is about to YIELD and will persist later in the
/// same iteration.
///
/// [`SessionManager::add_message_adopting_uid`] states the rule for the six
/// sites that persist BEFORE they yield — "the retained/yielded copy must carry
/// the same id as the stored row". The streaming path cannot persist first: the
/// model's reply is yielded the instant it arrives and can only be stored once
/// its tool calls have run. So it takes the inverse of the same rule — mint the
/// id *before* yielding, and let `add_message` store the row under the
/// caller-supplied id (it mints one only when the caller supplied none).
///
/// A fresh id per message, never a per-iteration one: a streamed reply that
/// arrives as several id-less chunks is stored as one row per chunk today
/// (`Conversation::push` merges only same-id neighbours, and `None == None` is
/// not a match there), and sharing one id across the chunks would silently
/// collapse them into a single row.
fn named(message: Message) -> Message {
    if message.id.is_some() {
        message
    } else {
        message.with_id(new_message_id())
    }
}

// #66 PERSISTED-ORDERING-SEAM:BEGIN
/// The only place an [`AgentEvent::MessagesPersisted`] can be built.
///
/// # The invariant
///
/// `MessagesPersisted` is an accounting frame: a client unions its ids into the
/// `expectedMessageIds` it hands `POST /sessions/{id}/edit_message`. Publishing
/// an id before the `Message` frame that carries it means a client that reads
/// the frame and then loses the stream — any send failure ends it, see
/// `routes/reply.rs` — claims **every** stored row while holding none of the
/// bodies that were still to come. The guard checks only `stored ∖ client`, so
/// that claim passes on a short transcript and the server truncates rows the
/// user can still see. (Over-reporting is the safe direction; under-reporting
/// is the one that deletes.)
///
/// The SSE adapter flushes buffered content before it forwards this frame for
/// exactly that reason, but it can only flush what it is *holding*: an order
/// emitted backwards here travels through it untouched.
///
/// The invariant, stated so it can be checked at every publication site: **no
/// `MessagesPersisted` may precede a `Message` frame carrying one of the ids it
/// publishes.**
///
/// # Why this is a module and not a helper (#66)
///
/// Until #66 the invariant was held by convention plus a test net: any site
/// could build the frame itself, and ordering it was opt-in. A new site got no
/// compile-time help and could violate it silently — which is not hypothetical.
/// Review found the inline slash-command path publishing before yielding, and
/// fixing it turned up two more nobody had reported: the hook-blocked prompt and
/// the elicitation seam.
///
/// So the builder is private to this module and the module's whole public
/// surface is three named constructors, one per legitimate shape. A site cannot
/// obtain the frame without saying which case it is, and "build it now, emit it
/// wherever" has no spelling. The audit is now a matter of reading the
/// constructor at each site rather than tracing control flow out from it.
///
/// # The three legitimate shapes
///
/// A blanket "always order it" rule would be wrong, because several sites
/// publish without yielding, for good reasons. The three shapes, and the
/// constructor that spells each:
///
///  1. [`yielded_then_named`] — **yield, then name.** The batch hands its
///     `Message` frames over and names rows in one breath, and the constructor
///     puts the content first. This is the unconditionally safe shape, and it
///     does not care whether the named rows are among the yielded ones — so a
///     site that merely *might* be case 1 should be case 1.
///  2. [`named_but_never_yielded`] — **names rows that no `Message` frame will
///     ever carry.** There is nothing for the accounting to arrive ahead of, so
///     ordering is vacuous; the rows are named only so `expectedMessageIds` can
///     be complete, and over-reporting is the safe direction. The caller must
///     state which audited reason applies ([`NeverYielded`]).
///  3. [`named_after_earlier_yield`] — **names rows already handed over earlier
///     in the same stream.** The `Message` frames went out before control
///     reached the call, so the obligation is discharged by then.
mod persisted_ordering {
    use super::{AgentEvent, Message};
    use serde::{Deserialize, Serialize};

    /// One row a turn persisted, as published by [`AgentEvent::MessagesPersisted`].
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct PersistedMessage {
        /// The `msg_uid` the row was actually stored under — the id
        /// `POST /sessions/{id}/edit_message` compares `expectedMessageIds`
        /// against.
        pub id: String,
        /// Whether this row is HIDDEN from the transcript. It is not a rendering
        /// instruction, and reading it as one double-draws.
        ///
        /// `false` is the model-only plumbing a turn stores but deliberately
        /// keeps out of the transcript (the BR-47 post-edit diagnostics, the
        /// loop-guard / stall / budget nudges, hook context). Publishing it
        /// *with* the flag is what separates "you are deliberately not being
        /// shown this row" from "you were never told it exists" — the client can
        /// name the id without drawing anything for it. That direction is exact:
        /// `false` means the row must not appear in the transcript.
        ///
        /// `true` means only "not hidden" — NOT "draw this". The content may
        /// already have been delivered inside a `Message` frame, and on a
        /// tool-bearing turn it has been: one streamed reply is stored as a
        /// rebuilt thinking row plus one `tool_use` row per request, each built
        /// from `Message::assistant()` / `Message::new` and so carrying the
        /// default `user_visible: true`, while the client was shown that same
        /// content once already as the reply itself. A client that drew every
        /// `true` row would render the same tool request twice.
        ///
        /// This frame is for ACCOUNTING — naming rows so `expectedMessageIds`
        /// can be complete. The transcript still comes from `Message` frames
        /// alone.
        pub user_visible: bool,
    }

    impl PersistedMessage {
        /// The published form of a message that has already adopted its
        /// effective uid. `None` for a message that still carries no id, which
        /// cannot be named and must not be claimed as published.
        ///
        /// Private to the seam: turning a `Message` into a published row is the
        /// step that has to be paired with an ordering decision, so it is not
        /// reachable from the publication sites. The struct's fields stay public
        /// because the server's SSE and relay tests build fixtures from them.
        fn of(message: &Message) -> Option<Self> {
            message.id.clone().map(|id| Self {
                id,
                user_visible: message.is_user_visible(),
            })
        }
    }

    /// The [`AgentEvent::MessagesPersisted`] for a batch of rows that have
    /// already adopted their effective uids, or `None` when there is nothing to
    /// publish.
    ///
    /// PRIVATE ON PURPOSE (#66). This is the step that must not be expressible
    /// on its own: with it in hand a site can emit the frame anywhere, including
    /// ahead of the content it names. Reach it through one of the three shapes.
    fn persisted_event<'a, I>(messages: I) -> Option<AgentEvent>
    where
        I: IntoIterator<Item = &'a Message>,
    {
        let published: Vec<PersistedMessage> = messages
            .into_iter()
            .filter_map(PersistedMessage::of)
            .collect();
        (!published.is_empty()).then_some(AgentEvent::MessagesPersisted(published))
    }

    /// Why a batch names rows that no `Message` frame will ever carry.
    ///
    /// A closed set, so shape 2 cannot be reached with a fresh excuse: a new
    /// reason costs a variant with a doc comment, which is where a reviewer gets
    /// to ask whether it is really a reason.
    #[derive(Clone, Copy, Debug)]
    pub(super) enum NeverYielded {
        /// The client's own prompt. It authored the row, so it already holds the
        /// body; the id is published because the *store* is what mints (or
        /// re-mints) it, and a client that never learns it under-reports.
        ClientAuthoredPrompt,
        /// Model-only plumbing: stored so the model sees it on the next provider
        /// call, deliberately kept out of the transcript. Loop-guard and stall
        /// nudges, wrap-up instructions, done-gate / self-critique / Stop-hook
        /// feedback.
        ModelOnly,
        /// A pre-stream batch mixing the two: the client's own prompt together
        /// with the model-only rows the same turn wrote before its stream
        /// existed (a slash command's resolution, injected hook context).
        ClientPromptAndModelOnly,
    }

    /// Shape 1 — **yield, then name.** The `Message` frames for `yielded`,
    /// followed by the one frame naming `named`.
    ///
    /// The ordering is the point: it is produced here, in a single call, so a
    /// site cannot come by the accounting frame without the content already
    /// sitting ahead of it.
    ///
    /// `yielded` and `named` are separate arguments because they are separate
    /// values even when they are the same rows: the stored copy carries the
    /// visibility it was written with, and the published `user_visible` flag has
    /// to describe the STORED row, not the copy handed to the client.
    ///
    /// Nothing to yield means this is really shape 2 or 3 — say which.
    pub(super) fn yielded_then_named<'a>(
        yielded: impl IntoIterator<Item = Message>,
        named: impl IntoIterator<Item = &'a Message>,
    ) -> Vec<AgentEvent> {
        let mut events: Vec<AgentEvent> = yielded.into_iter().map(AgentEvent::Message).collect();
        debug_assert!(
            !events.is_empty(),
            "`yielded_then_named` with nothing to yield is `named_but_never_yielded` \
             or `named_after_earlier_yield`; the shape has to be stated, not \
             defaulted to whichever one looks safest"
        );
        events.extend(persisted_event(named));
        events
    }

    /// Shape 2 — **names rows no `Message` frame will ever carry.**
    ///
    /// Ordering is vacuous here: there is no frame for the accounting to arrive
    /// ahead of. The rows are named anyway so `expectedMessageIds` can be
    /// complete — over-reporting is safe, under-reporting is what deletes.
    ///
    /// `why` is the audited reason. It is not read on the hot path; it exists so
    /// that a new site has to name its case, and so the claim is reviewable at
    /// the call site instead of being reconstructible only by tracing the
    /// stream.
    pub(super) fn named_but_never_yielded<'a>(
        named: impl IntoIterator<Item = &'a Message>,
        why: NeverYielded,
    ) -> Option<AgentEvent> {
        let event = persisted_event(named);
        if let Some(AgentEvent::MessagesPersisted(rows)) = &event {
            tracing::trace!(
                rows = rows.len(),
                reason = ?why,
                "naming rows no `Message` frame will carry"
            );
        }
        event
    }

    /// Shape 3 — **names rows already handed over earlier in this stream.**
    ///
    /// The `Message` frames went out before control reached here, so the
    /// obligation is discharged by the time this is called. Use it only where
    /// the yields provably precede the call in the same stream; where the yields
    /// happen in the same breath, that is shape 1 and the ordering should be
    /// mechanical rather than argued.
    pub(super) fn named_after_earlier_yield<'a>(
        named: impl IntoIterator<Item = &'a Message>,
    ) -> Option<AgentEvent> {
        persisted_event(named)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Every id named by a `MessagesPersisted` that a **later** `Message`
        /// frame turns out to carry — i.e. every id handed over before its body.
        /// The invariant is exactly "this is empty".
        fn ids_named_before_their_message(events: &[AgentEvent]) -> Vec<String> {
            let mut named: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut early = Vec::new();
            for event in events {
                match event {
                    AgentEvent::MessagesPersisted(rows) => {
                        named.extend(rows.iter().map(|row| row.id.clone()));
                    }
                    AgentEvent::Message(message) => {
                        if let Some(id) = message.id.as_ref().filter(|id| named.contains(*id)) {
                            early.push(id.clone());
                        }
                    }
                    _ => {}
                }
            }
            early
        }

        fn row(text: &str, id: &str) -> Message {
            Message::user().with_text(text).with_id(id)
        }

        #[test]
        fn shape_one_puts_every_body_ahead_of_the_frame_that_names_it() {
            let first = row("first", "id-1");
            let second = row("second", "id-2");

            let events = yielded_then_named([first.clone(), second.clone()], [&first, &second]);

            assert!(
                matches!(events.as_slice(), [_, _, AgentEvent::MessagesPersisted(_)]),
                "content first, accounting last: {events:#?}"
            );
            assert!(
                ids_named_before_their_message(&events).is_empty(),
                "shape 1 must never name an id ahead of its body: {events:#?}"
            );
        }

        /// The check above is worthless if it cannot fail. Reversing the same
        /// batch is the defect #66 exists to make inexpressible, so the checker
        /// has to catch it.
        #[test]
        fn the_ordering_check_catches_the_reversed_batch() {
            let only = row("only", "id-1");
            let mut events = yielded_then_named([only.clone()], std::slice::from_ref(&only));
            events.reverse();

            assert_eq!(
                ids_named_before_their_message(&events),
                vec!["id-1".to_string()],
                "a batch that names before it yields must be flagged"
            );
        }

        /// A row that never adopted a uid cannot be named — claiming it would
        /// hand the client an id no row answers to.
        #[test]
        fn an_id_less_row_is_not_claimed() {
            let anonymous = Message::user().with_text("no id yet");

            assert!(
                named_after_earlier_yield(std::slice::from_ref(&anonymous)).is_none(),
                "nothing to publish means no frame at all"
            );
            assert_eq!(
                yielded_then_named([anonymous.clone()], std::slice::from_ref(&anonymous)).len(),
                1,
                "the body still goes over the wire; only the accounting is skipped"
            );
        }

        /// Shapes 2 and 3 publish the same frame — they differ only in the claim
        /// the caller is making about the stream around them.
        #[test]
        fn shape_two_and_three_name_the_rows_they_are_given() {
            let hidden = row("model-only", "id-h");

            for event in [
                named_but_never_yielded(std::slice::from_ref(&hidden), NeverYielded::ModelOnly),
                named_after_earlier_yield(std::slice::from_ref(&hidden)),
            ] {
                let Some(AgentEvent::MessagesPersisted(rows)) = event else {
                    panic!("a row with an id must be named");
                };
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].id, "id-h");
            }
        }
    }
}

pub use persisted_ordering::PersistedMessage;
use persisted_ordering::{
    named_after_earlier_yield, named_but_never_yielded, yielded_then_named, NeverYielded,
};
// #66 PERSISTED-ORDERING-SEAM:END

/// Injected in place of a selected skill's full body on any turn after the first
/// it was loaded (BR-8), so a skill-heavy session doesn't re-inline the whole
/// body every turn.
fn skill_already_loaded_pointer() -> &'static str {
    "This skill's full instructions were already loaded earlier in this session, so they are not \
     repeated here to save context. They remain in effect; call the `skills__loadSkill` tool to \
     re-read the full text if you need it again."
}
// NOTE: the "continue when the agent stops with unchecked todos" behavior used to
// live here as a hard-coded agent-loop completion gate that fabricated a *visible*
// `user` message every turn. That over-reached: when the agent was genuinely stuck
// (e.g. an unrecoverable provider error), it re-injected the same message forever
// and never resolved the root cause — and it polluted the conversation with fake
// user input. "Don't stop while work is unfinished" is now left to the proper,
// bounded, user-configurable mechanisms: the Stop-hook system (`StopHookVerdict`,
// capped by `STOP_HOOK_BLOCK_CAP`, delivered as hidden-visibility feedback + a
// user-facing system notification) and the `/goal` loop (whose stall budget does
// NOT reset when tools run, so it gives up when progress stalls). A user who wants
// "keep going until the todos are done" sets a `/goal` or a Stop hook — both go
// through that bounded, stall-aware path instead of an unbounded loop injection.

/// Context needed for the reply function
pub struct ReplyContext {
    pub conversation: Conversation,
    pub tools: Vec<Tool>,
    pub toolshim_tools: Vec<Tool>,
    pub system_prompt: String,
    pub biorouter_mode: BioRouterMode,
    /// The transcript as it stood before the turn started — the snapshot the retry
    /// path restores from. A `Conversation` (not a `Vec<Message>`) so taking it is
    /// a refcount bump rather than a deep copy of the history (BR-56).
    pub initial_messages: Conversation,
}

/// The freshness basis for a whole-history rewrite: a durable revision together
/// with the history that was read AT it.
///
/// `replace_conversation_preserving_tail` splits the stored rows at
/// `basis.max_rowid`. Rows ABOVE the watermark that `known` does not name belong
/// to another writer and are carried over; everything at or below it is deleted
/// and replaced. That is sound only when the two halves come from ONE paired
/// read — revision first, then the conversation, which is exactly what
/// [`SessionManager::snapshot_for_rewrite`] guarantees. Source them
/// independently and an append landing in between is counted by
/// `basis.max_rowid` — so `scan_foreign_tail`, which only scans above the
/// watermark, never sees it — while `known` does not contain it either. The
/// DELETE then destroys it, after `add_message` already told that writer it had
/// succeeded. That is the whole bug the guard exists to prevent, reintroduced
/// one level up.
///
/// So the two halves live in one value that can ONLY be produced by a paired
/// read: there is no constructor taking a revision and a conversation, and the
/// fields are private. A caller cannot re-split them, and "re-seed the basis"
/// cannot compile into "re-seed the revision".
struct RewriteBasis {
    /// The stored history at `revision` — the seed the turn's live conversation
    /// descends from.
    known: Conversation,
    revision: crate::session::session_manager::ConversationRevision,
}

impl RewriteBasis {
    /// The only way to obtain one.
    async fn read(session_manager: &SessionManager, session_id: &str) -> Result<Self> {
        Ok(Self::read_with_session(session_manager, session_id)
            .await?
            .1)
    }

    /// The same paired read, keeping the `Session` it came from so a caller that
    /// needs the session metadata does not pay for a second round trip — nor
    /// get it from a *different* moment than the basis.
    async fn read_with_session(
        session_manager: &SessionManager,
        session_id: &str,
    ) -> Result<(Session, Self)> {
        let (session, revision) = session_manager.snapshot_for_rewrite(session_id).await?;
        let known = session
            .conversation
            .clone()
            .ok_or_else(|| anyhow!("Session {session_id} has no conversation"))?;
        Ok((session, Self { known, revision }))
    }

    /// The stored history this basis was read with.
    fn known(&self) -> &Conversation {
        &self.known
    }

    fn raw_with_new_durable_messages(&self, live: &Conversation) -> Conversation {
        let mut seen_ids = self
            .known
            .iter()
            .filter_map(|message| message.id.clone())
            .collect::<HashSet<_>>();
        let mut messages = self.known.messages().clone();
        messages.extend(live.iter().filter_map(|message| {
            let id = message.id.as_ref()?;
            seen_ids.insert(id.clone()).then(|| message.clone())
        }));
        Conversation::new_unvalidated(messages)
    }
}

/// What one overflow-recovery compaction achieved.
struct OverflowCompactionSwap {
    /// The conversation now on disk — `None` when the swap could not be made
    /// safely, in which case the store is untouched and `compacted` is adopted
    /// in memory only so the turn can still make progress.
    stored: Option<Conversation>,
    /// The compaction that was computed, persisted or not.
    compacted: Conversation,
    /// Every summarization round-trip this made, oldest first. All of it is
    /// spend (BR-35) whether or not the result was kept.
    usages: Vec<crate::providers::base::ProviderUsage>,
}

impl OverflowCompactionSwap {
    /// The compaction could not be persisted — the store errored, the basis
    /// could not be re-read, or the retry was declined too.
    ///
    /// The turn adopts `compacted` in memory anyway so it can still finish, and
    /// `usages` rides along so already-billed spend is still reported. Every
    /// give-up path in this file goes through here, so none of them can
    /// accidentally drop the spend on its way out.
    fn unpersisted(
        compacted: Conversation,
        usages: Vec<crate::providers::base::ProviderUsage>,
    ) -> Self {
        Self {
            stored: None,
            compacted,
            usages,
        }
    }
}

/// BR-56: normalize the transcript before every provider call, not just once per
/// reply. Kill switch: `BIOROUTER_NORMALIZE_EACH_TURN=false`.
fn normalize_each_turn() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_NORMALIZE_EACH_TURN")
        .unwrap_or(true)
}

/// BR-71 decision 22: the merged spawn tool reaches dispatch under the
/// workspace prefix, and bare for models that strip prefixes (the same
/// tolerance `ExtensionManager::dispatch_tool_call` already applies to
/// code_execution tools).
pub(crate) fn is_spawn_tool_call(tool_name: &str) -> bool {
    tool_name == crate::agents::subagent_tool::SUBAGENT_TOOL_PREFIXED
        || tool_name == crate::agents::subagent_tool::SUBAGENT_TOOL_NAME
}

/// BR-71 §5 + decision 25: subagents never get workspace control — no
/// delegation-tree fan-out of cross-session control, no child steering its
/// parent, and (since the spawn tool is now a workspace tool) no nesting.
///
/// Name forms: extension-advertised tools reach dispatch PREFIXED
/// (`workspace__workspace_list`; the `format!("{}__{}", name, tool.name)` that
/// makes the name lives in `extension_manager.rs`, behind the
/// `config.is_tool_available` filter), and the bare forms cover prefix-stripping
/// models (the `if !tool_name_str.contains("__")` block in
/// `extension_manager.rs` that re-prefixes the three known `code_execution`
/// tools is the precedent). The spawn tool is covered separately because it is
/// named `subagent`, not `workspace_*`.
///
/// The names are ENUMERATED rather than prefix-matched. A bare
/// `tool_name.starts_with("workspace_")` also matches any third-party extension
/// whose *name* begins with `workspace_` — its tools arrive as
/// `workspace_foo__bar`, which starts with `workspace_` — and every one of them
/// would be refused inside a subagent with the misleading message "Subagents
/// cannot use workspace tools." An explicit list cannot do that, and it is a
/// closed set we control: when a `workspace_*` tool is added, this list is where
/// the compiler-free reminder lives. It is not left to a reminder, though:
/// `the_refusal_list_mirrors_every_tool_the_workspace_extension_advertises`
/// cross-checks it against `WorkspaceClient::get_tools()` in both directions, so
/// an eighth tool added there fails this crate's suite instead of quietly
/// becoming reachable inside a delegation tree.
const WORKSPACE_TOOL_NAMES: [&str; 7] = [
    "workspace_list",
    "workspace_open",
    "workspace_read_conversation",
    "workspace_send_prompt",
    "workspace_set_tools",
    "workspace_close",
    "workspace_watch",
];

pub(crate) fn is_workspace_tool_refused_for(
    session_type: crate::session::session_manager::SessionType,
    tool_name: &str,
) -> bool {
    if session_type != crate::session::session_manager::SessionType::SubAgent {
        return false;
    }
    if is_spawn_tool_call(tool_name) {
        return true;
    }
    // Bare, or prefixed by OUR extension — not by anything that merely starts
    // with the same letters.
    let bare = tool_name.strip_prefix("workspace__").unwrap_or(tool_name);
    WORKSPACE_TOOL_NAMES.contains(&bare)
}

/// Workspace tools that block on work happening in ANOTHER session, and must
/// therefore not hold a global tool-dispatch permit while they do. Both name
/// forms, like `is_spawn_tool_call`.
pub(crate) fn is_parking_workspace_tool(name: &str) -> bool {
    matches!(
        name,
        "workspace_watch"
            | "workspace__workspace_watch"
            | "workspace_send_prompt"
            | "workspace__workspace_send_prompt"
    )
}

pub struct ToolCategorizeResult {
    pub frontend_requests: Vec<ToolRequest>,
    pub remaining_requests: Vec<ToolRequest>,
    pub filtered_response: Message,
}

/// The per-batch lookup tables one round of tool calls needs, built in one
/// place by [`build_tool_request_maps`] and destructured straight back into the
/// locals the reply loop reads.
///
/// This exists for stack, not for tidiness. `reply_internal` returns a single
/// ~2000-line `try_stream!` generator, and at `opt-level = 0` every temporary
/// in a generator body gets its own slot in that generator's `poll` frame with
/// no reuse between them. A subagent reply nests another whole copy of the
/// frame underneath the parent's, so the frame is what decides how deep
/// delegation can go before the thread's stack is gone (issue #87: three
/// children were enough). Every yield-free block that moves out of the
/// generator and into a function of its own gets a frame that is only live
/// while it runs. Please do not inline these back.
struct ToolBatchMaps {
    tool_response_messages: Vec<Arc<Mutex<Message>>>,
    request_to_response_map: HashMap<String, Arc<Mutex<Message>>>,
    request_metadata: HashMap<String, Option<ProviderMetadata>>,
    request_to_original_tool_call: HashMap<String, CallToolRequestParams>,
    request_to_executed_tool_call: HashMap<String, CallToolRequestParams>,
    request_to_tool_name: HashMap<String, String>,
}

/// Build the per-batch tool lookup tables and the response slots they index.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn build_tool_request_maps(
    response: &Message,
    frontend_requests: &[ToolRequest],
    remaining_requests: &[ToolRequest],
) -> ToolBatchMaps {
    let num_tool_requests = frontend_requests.len() + remaining_requests.len();
    let tool_response_messages: Vec<Arc<Mutex<Message>>> = (0..num_tool_requests)
        .map(|_| {
            Arc::new(Mutex::new(
                Message::user().with_id(format!("msg_{}", Uuid::new_v4())),
            ))
        })
        .collect();

    let mut request_to_response_map = HashMap::new();
    let mut request_metadata: HashMap<String, Option<ProviderMetadata>> = HashMap::new();
    let request_to_original_tool_call = response
        .content
        .iter()
        .filter_map(|content| match content {
            MessageContent::ToolRequest(request) => request
                .tool_call
                .as_ref()
                .ok()
                .map(|tool_call| (request.id.clone(), tool_call.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut request_to_executed_tool_call = HashMap::new();
    // The tool that produced each result, for the untrusted-data frame's
    // provenance attribute. Built here rather than looked up later because this
    // is the one place the request and its id are together; a malformed request
    // has no name, and the frame says `unknown` rather than being skipped.
    let mut request_to_tool_name: HashMap<String, String> = HashMap::new();
    for (idx, request) in frontend_requests
        .iter()
        .chain(remaining_requests.iter())
        .enumerate()
    {
        request_to_response_map.insert(request.id.clone(), tool_response_messages[idx].clone());
        request_metadata.insert(request.id.clone(), request.metadata.clone());
        if let Ok(tool_call) = &request.tool_call {
            request_to_tool_name.insert(request.id.clone(), tool_call.name.to_string());
            request_to_executed_tool_call.insert(request.id.clone(), tool_call.clone());
        }
    }

    ToolBatchMaps {
        tool_response_messages,
        request_to_response_map,
        request_metadata,
        request_to_original_tool_call,
        request_to_executed_tool_call,
        request_to_tool_name,
    }
}

/// Stamp `biorouterToolExecution` onto a frontend tool's response when the
/// arguments that actually ran differ from the ones the provider authored.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
async fn note_tool_argument_rewrite(
    request_id: &str,
    request_to_original_tool_call: &HashMap<String, CallToolRequestParams>,
    request_to_executed_tool_call: &HashMap<String, CallToolRequestParams>,
    request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
) {
    let (Some(original), Some(executed), Some(response_slot)) = (
        request_to_original_tool_call.get(request_id),
        request_to_executed_tool_call.get(request_id),
        request_to_response_map.get(request_id),
    ) else {
        return;
    };
    let original_value = serde_json::to_value(original).ok();
    let executed_value = serde_json::to_value(executed).ok();
    if original_value == executed_value {
        return;
    }
    let mut response = response_slot.lock().await;
    for content in &mut response.content {
        let MessageContent::ToolResponse(tool_response) = content else {
            continue;
        };
        let audit = serde_json::json!({
            "providerAuthored": original_value,
            "actuallyExecuted": executed_value,
        });
        match &mut tool_response.tool_result {
            Ok(result) => {
                let meta = result.meta.get_or_insert_with(rmcp::model::Meta::new);
                meta.0.insert("biorouterToolExecution".to_string(), audit);
            }
            Err(error) => {
                let mut data = match error.data.take() {
                    Some(Value::Object(data)) => data,
                    Some(data) => {
                        serde_json::Map::from_iter([("providerErrorData".to_string(), data)])
                    }
                    None => serde_json::Map::new(),
                };
                data.insert("biorouterToolExecution".to_string(), audit);
                error.data = Some(Value::Object(data));
            }
        }
    }
}

/// Everything one reply loop resolves from config exactly once, before its first
/// iteration.
///
/// Each of these reads the filesystem, so they are resolved once per reply
/// rather than per turn or per tool result. They live in a struct the generator
/// destructures because a `from_config()` call materialises its result in the
/// caller's frame first — and in the `reply_internal` generator that frame is
/// the one issue #87 is about. See [`ToolBatchMaps`].
struct ReplyLoopPolicy {
    /// BR-63: the effort-scaled exploration budget — `quick` halves it (never
    /// below a usable floor, never above what the user configured), `deep`
    /// doubles it, `normal` leaves it exactly as configured.
    max_turns: u32,
    /// Cumulative tool calls one reply may dispatch, across all iterations, so
    /// parallel fan-out cannot run unbounded while `turns_taken` stays under
    /// `max_turns`.
    max_tool_calls: u32,
    tool_output_guardrail: crate::guardrails::tool_output::ToolOutputGuardrailMode,
    /// BR-51: the tool-error taxonomy policy.
    tool_error_taxonomy: crate::agents::tool_errors::ToolErrorTaxonomyConfig,
    /// BR-47: the auto post-edit diagnostics policy.
    post_edit_diag_config: crate::agents::post_edit_diagnostics::PostEditDiagnosticsConfig,
    /// BR-50: the optional self-critique / reflection policy. Default OFF; when
    /// a user opts in it re-reads an ordinary answer for correctness before it
    /// is returned, using the goal-judge LLM primitive.
    self_critique_config: crate::agents::self_critique::SelfCritiqueConfig,
    /// BR-48: the optional deterministic done-ness gate. Default OFF; when a
    /// user opts in it re-runs their `SuccessCheck`s before the turn may finish
    /// and keeps the agent working on the failures.
    done_gate_config: crate::agents::done_gate::DoneGateConfig,
    /// BR-32: the /goal stall detector, generalized to ordinary chat.
    stall_config: crate::agents::stall::StallCheckConfig,
    /// BR-66: the general mistake streak — consecutive failed tool calls of
    /// *any* kind (BR-31 only sees one tool failing one way), plus the
    /// recoverable-provider-error counter that decides whether a failed model
    /// call ends the turn or earns one more attempt with a hint.
    mistake_config: crate::agents::mistakes::MistakeConfig,
    /// BR-35: the per-reply wall-clock / token / dollar ceiling. `max_turns` and
    /// `max_tool_calls` bound how many *steps* a reply takes, which is not a
    /// bound on time or money — 429 backoff (~2 min/call) compounds inside a
    /// single step, and one step can re-bill a 200k-token context. Inert (and
    /// free) unless a limit is configured; a limit set on the session wins
    /// per-axis over the global config.
    budget: BudgetTracker,
}

/// Resolve [`ReplyLoopPolicy`] for one reply.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn resolve_reply_loop_policy(
    effort: ReasoningEffort,
    session_config: &SessionConfig,
) -> ReplyLoopPolicy {
    ReplyLoopPolicy {
        max_turns: effort.scale_turns(
            session_config
                .max_turns
                .or_else(|| Config::global().get_param("BIOROUTER_MAX_TURNS").ok())
                .unwrap_or(DEFAULT_MAX_TURNS),
        ),
        max_tool_calls: effort.scale_tool_calls(
            session_config
                .max_tool_calls
                .or_else(|| Config::global().get_param("BIOROUTER_MAX_TOOL_CALLS").ok())
                .unwrap_or(DEFAULT_MAX_TOOL_CALLS),
        ),
        tool_output_guardrail: crate::guardrails::tool_output::ToolOutputGuardrailMode::from_config(
        ),
        tool_error_taxonomy: crate::agents::tool_errors::ToolErrorTaxonomyConfig::from_config(),
        post_edit_diag_config:
            crate::agents::post_edit_diagnostics::PostEditDiagnosticsConfig::from_config(),
        self_critique_config: crate::agents::self_critique::SelfCritiqueConfig::from_config(),
        done_gate_config: crate::agents::done_gate::DoneGateConfig::from_config(),
        stall_config: crate::agents::stall::StallCheckConfig::from_config(Config::global()),
        mistake_config: crate::agents::mistakes::MistakeConfig::from_config(Config::global()),
        budget: BudgetTracker::new(ReplyBudget::resolve(
            session_config.budget,
            Config::global(),
        )),
    }
}

/// Emit one loop-safety event.
///
/// A function rather than the builder chain at each of the reply loop's fifteen
/// call sites: every intermediate `LoopSafetyEvent` in a chain is its own slot
/// in the `reply_internal` generator's `poll` frame (issue #87). See
/// [`ToolBatchMaps`].
fn emit_loop_safety(
    kind: LoopSafetyKind,
    session_id: &str,
    count: u32,
    limit: Option<u32>,
    axis: Option<&str>,
) {
    let mut event = LoopSafetyEvent::new(kind).session(session_id).count(count);
    if let Some(limit) = limit {
        event = event.limit(limit);
    }
    loop_safety::emit(event.maybe_axis(axis));
}

/// The Pre/PostToolUse hook context staged by the inspector and the permission
/// gate: the inline notices to surface, and the single model-visible context row
/// they add up to.
///
/// BR-19: both sites stage their `additionalContext` / `systemMessage` because
/// neither return channel can carry them — both used to read only the decision
/// and drop the rest. Drained once both have run: messages surface as inline
/// notices, context reaches the model with the same untrusted framing (BR-26) as
/// the SessionStart / UserPromptSubmit path.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn staged_tool_hook_context(
    staged: Vec<crate::hooks::StagedToolHook>,
) -> (Vec<Message>, Option<Message>) {
    let mut notices = Vec::new();
    let mut hook_contexts: Vec<String> = Vec::new();
    for entry in staged {
        for msg in entry.system_messages {
            notices.push(
                Message::assistant()
                    .with_system_notification(SystemNotificationType::InlineMessage, msg)
                    .user_only(),
            );
        }
        hook_contexts.extend(entry.additional_context);
    }
    (notices, hook_context_message(&hook_contexts))
}

/// The one model-visible row a batch of hook `additionalContext` strings becomes,
/// under BR-26's untrusted framing. `None` when nothing was staged.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn hook_context_message(hook_contexts: &[String]) -> Option<Message> {
    if hook_contexts.is_empty() {
        return None;
    }
    Some(
        Message::user()
            .with_text(crate::hooks::outcome::frame_hook_context(
                &hook_contexts.join("\n\n"),
            ))
            .with_visibility(false, true),
    )
}

/// A user-visible inline system notice.
///
/// Named constructors rather than builder chains at the call site: in the
/// `reply_internal` generator every intermediate `Message` in a chain is its own
/// 128-byte slot in that generator's `poll` frame (issue #87), so a three-call
/// chain costs three where one call costs one. Please build these through the
/// constructors rather than re-inlining the chains. See [`ToolBatchMaps`].
fn inline_notice<S: Into<String>>(text: S) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::InlineMessage, text)
}

/// [`inline_notice`], hidden from the model.
fn inline_notice_user_only<S: Into<String>>(text: S) -> Message {
    inline_notice(text).user_only()
}

/// The transient "thinking" notice shown while a compaction runs.
fn thinking_notice<S: Into<String>>(text: S) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::ThinkingMessage, text)
}

/// A plain assistant message carrying `text`.
fn assistant_text<S: Into<String>>(text: S) -> Message {
    Message::assistant().with_text(text)
}

/// A user-role row the model sees but the transcript does not show.
fn model_only_user_text<S: Into<String>>(text: S) -> Message {
    Message::user().with_text(text).with_visibility(false, true)
}

/// [`model_only_user_text`] under a freshly minted message id.
fn model_only_user_text_with_new_id<S: Into<String>>(text: S) -> Message {
    Message::user()
        .with_id(format!("msg_{}", Uuid::new_v4()))
        .with_text(text)
        .with_visibility(false, true)
}

/// Persist one loop iteration's messages and hand them back carrying the uids
/// the store actually used.
///
/// #41: duplicate ids are re-minted before the write — a decoder that reused one
/// id across several yielded messages would otherwise fail the
/// `UNIQUE(session_id, msg_uid)` index and kill the turn. Each row then adopts
/// the EFFECTIVE uid it was persisted under: on a collision the store re-mints,
/// and the in-memory conversation must carry the same id as the row, or the next
/// persist of that message duplicates it under a stale id.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
async fn persist_iteration_messages(
    session_manager: &SessionManager,
    session_id: &str,
    messages_to_add: Conversation,
) -> Result<Vec<Message>> {
    let mut messages_to_add = remint_duplicate_message_ids(messages_to_add).into_messages();
    for msg in &mut messages_to_add {
        let effective_uid = session_manager.add_message(session_id, msg).await?;
        if msg.id.as_deref() != Some(effective_uid.as_str()) {
            msg.id = Some(effective_uid);
        }
    }
    Ok(messages_to_add)
}

/// The message one queued soft interrupt becomes.
///
/// Frames ONLY agent-originated steers. The drain loop this feeds is SHARED
/// with the human's own typed soft interrupt, which `queue_soft_interrupt`
/// enqueues with `provenance: None` — framing that unconditionally would wrap
/// the user's own words in an untrusted envelope and tell the model to discount
/// them.
///
/// The inner match is EXHAUSTIVE over `ProvenanceKind` on purpose, and must stay
/// that way. `ProvenanceKind` is not `#[non_exhaustive]`, so a `_` catch-all
/// here would let a variant added later fall silently into the *unframed* arm —
/// putting cross-session agent text into this session's context without the
/// untrusted envelope, which is the exact prompt-injection vector
/// `frame_workspace_injection` exists to close. Exhaustiveness makes a new
/// variant a compile error and forces an explicit framing decision.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn soft_interrupt_message(queued: QueuedInterrupt) -> Message {
    let QueuedInterrupt { text, provenance } = queued;
    let body = match &provenance {
        Some(p) => match p.kind {
            ProvenanceKind::AgentInjection => {
                crate::conversation::message::frame_workspace_injection(
                    p.from_session_name.as_deref(),
                    &text,
                )
            }
            // The human typed this into the subagent's own tab, or it is a
            // spawn-context record this session authored — neither is another
            // agent's text, so neither is framed.
            ProvenanceKind::UserDirect | ProvenanceKind::SpawnContext => text,
        },
        // Unstamped: the human's own typed soft interrupt.
        None => text,
    };
    let mut m = Message::user().with_text(body);
    if let Some(p) = provenance {
        m = m.with_provenance(p);
    }
    m
}

/// Persist a model-only steering row (a stall nudge, a budget wrap-up, a
/// done-gate or self-critique instruction, Stop-hook feedback) and hand back the
/// row plus the `#59` / `#66 SHAPE 2` frame naming it — a row the user is
/// deliberately not shown, named so a client can still account for it.
///
/// The caller yields the frame and folds the row into its conversation, so the
/// order at each call site is unchanged.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
async fn persist_steering_message(
    session_manager: &SessionManager,
    session_id: &str,
    text: String,
) -> Result<(Message, Option<AgentEvent>)> {
    let mut steer = Message::user().with_text(text).with_visibility(false, true);
    session_manager
        .add_message_adopting_uid(session_id, &mut steer)
        .await?;
    let published = named_but_never_yielded(std::slice::from_ref(&steer), NeverYielded::ModelOnly);
    Ok((steer, published))
}

/// The ids of this signed turn's tool requests that have an executable
/// counterpart and a response slot, in the provider-authored order the signed
/// assistant block fixes.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn signed_turn_paired_response_ids(
    response: &Message,
    frontend_requests: &[ToolRequest],
    remaining_requests: &[ToolRequest],
    request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
) -> Vec<String> {
    response
        .content
        .iter()
        .filter_map(|content| match content {
            MessageContent::ToolRequest(request) => Some(request),
            _ => None,
        })
        .filter(|original| {
            let Some(executed) = frontend_requests
                .iter()
                .chain(remaining_requests.iter())
                .find(|request| request.id == original.id)
            else {
                return false;
            };
            executed.tool_call.is_ok() && request_to_response_map.contains_key(&original.id)
        })
        .map(|original| original.id.clone())
        .collect()
}

/// The assistant-side rows an *unsigned* provider's turn is persisted as: the
/// thinking row (if the reply carried any), then one `tool_use` row per
/// executable request paired with the index of its response slot.
///
/// Providers without signed assistant content keep the established transcript
/// shape — the normalized executable request is paired immediately with its
/// result — so the ids are minted here in exactly the order the caller pushes
/// them.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn unsigned_turn_assistant_rows(
    response: &Message,
    frontend_requests: &[ToolRequest],
    remaining_requests: &[ToolRequest],
) -> (Option<Message>, Vec<(usize, String, Message)>) {
    let mut assistant_turn_id = Some(assistant_turn_message_id(response));
    let mut next_assistant_id = move || {
        assistant_turn_id
            .take()
            .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4()))
    };

    let thinking_content: Vec<MessageContent> = response
        .content
        .iter()
        .filter(|content| {
            matches!(
                content,
                MessageContent::Thinking(_) | MessageContent::RedactedThinking(_)
            )
        })
        .cloned()
        .collect();
    let thinking_row = (!thinking_content.is_empty()).then(|| {
        Message::new(response.role.clone(), response.created, thinking_content)
            .with_id(next_assistant_id())
    });

    let mut tool_rows = Vec::new();
    for (idx, request) in frontend_requests
        .iter()
        .chain(remaining_requests.iter())
        .enumerate()
    {
        if request.tool_call.is_ok() {
            tool_rows.push((
                idx,
                request.id.clone(),
                Message::assistant()
                    .with_id(next_assistant_id())
                    .with_tool_request_with_metadata(
                        request.id.clone(),
                        request.tool_call.clone(),
                        request.metadata.as_ref(),
                        request.tool_meta.clone(),
                    ),
            ));
        }
    }
    (thinking_row, tool_rows)
}

/// Append the model-only rows this tool batch staged: BR-47's post-edit syntax
/// diagnostics (placed right after the tool responses for the edits they
/// describe), the Pre/PostToolUse hook context, and the BR-29/30/31 loop-guard
/// warnings. All model-visible only — corrective plumbing the user does not need
/// in the transcript.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn push_staged_batch_rows(
    messages_to_add: &mut Conversation,
    pending_post_edit_diagnostics: Option<String>,
    pending_pre_tool_hook_context: Option<Message>,
    pending_post_tool_hook_context: Option<Message>,
    loop_warnings: &[String],
) {
    if let Some(diagnostics_text) = pending_post_edit_diagnostics {
        messages_to_add.push(
            Message::user()
                .with_id(format!("msg_{}", Uuid::new_v4()))
                .with_text(diagnostics_text)
                .with_visibility(false, true),
        );
    }
    if let Some(context_message) = pending_pre_tool_hook_context {
        messages_to_add.push(context_message);
    }
    if let Some(context_message) = pending_post_tool_hook_context {
        messages_to_add.push(context_message);
    }
    // Soft stage (BR-29/30/31): the repeated — or repeatedly failing — call
    // *ran*; nudge the model right after its result so it changes approach
    // before the hard stop fires.
    if !loop_warnings.is_empty() {
        tracing::info!(
            warnings = loop_warnings.len(),
            "Injecting loop-guard soft warning"
        );
        messages_to_add.push(
            Message::user()
                .with_id(format!("msg_{}", Uuid::new_v4()))
                .with_text(crate::tool_inspection::frame_loop_warnings(loop_warnings))
                .with_visibility(false, true),
        );
    }
}

/// The terminal notice for a reply whose output-length continuations ran out.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn truncation_exhausted_notice(
    zero_progress: bool,
    ever_made_user_visible_progress: bool,
    continuations: u32,
) -> String {
    if zero_progress && !ever_made_user_visible_progress {
        format!(
            "The model repeatedly reached its output limit without producing a user-visible answer, so BioRouter stopped automatic continuation after {continuations} attempts. No partial answer was available to preserve. You can retry or ask me to continue in a new message."
        )
    } else {
        format!(
            "The model repeatedly reached its output limit across this reply, so BioRouter stopped automatic continuation after {continuations} attempts. The partial response above has been preserved. You can ask me to continue in a new message."
        )
    }
}

/// Run the PostToolUse / PostToolUseFailure hooks for one completed tool batch
/// and return each one's aggregate, in dispatch order.
///
/// Awaited (rather than fired and forgotten) so the injected context lands
/// before the next provider call, and (BR-19) so the decision can be honored —
/// a `block` turns the result into corrective feedback instead of being
/// computed and thrown away. The caller applies the block, bounded by
/// `POST_TOOL_HOOK_BLOCK_CAP` so a hook that always blocks cannot wedge the
/// turn.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
async fn dispatch_post_tool_hooks(
    hooks: Arc<crate::hooks::HooksManager>,
    session_id: &str,
    working_dir: &std::path::Path,
    post_tool_results: Vec<(String, Option<Value>, Option<String>)>,
    remaining_requests: &[ToolRequest],
) -> Vec<(String, String, crate::hooks::HookAggregate)> {
    let mut post_futures = Vec::new();
    for (request_id, response_value, error_text) in post_tool_results {
        let Some(request) = remaining_requests.iter().find(|r| r.id == request_id) else {
            continue;
        };
        let Ok(tool_call) = &request.tool_call else {
            continue;
        };
        let tool_name = tool_call.name.to_string();
        let event = if error_text.is_some() {
            crate::hooks::HookEvent::PostToolUseFailure
        } else {
            crate::hooks::HookEvent::PostToolUse
        };
        if !hooks.has_hooks(event, Some(&tool_name), working_dir).await {
            continue;
        }
        let mut payload =
            crate::hooks::HookPayload::new(event, session_id, working_dir.to_string_lossy());
        payload.tool_name = Some(tool_name.clone());
        payload.tool_input = Some(
            tool_call
                .arguments
                .clone()
                .map(Value::Object)
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        );
        payload.tool_response = response_value;
        payload.error = error_text;
        let hooks = Arc::clone(&hooks);
        let working_dir = working_dir.to_path_buf();
        post_futures.push(async move {
            let aggregate = hooks
                .dispatch(event, Some(&tool_name), &payload, &working_dir)
                .await;
            (request_id, tool_name, aggregate)
        });
    }
    if post_futures.is_empty() {
        return Vec::new();
    }
    futures::future::join_all(post_futures).await
}

/// The terminal abort a repetition-policy denial produces, if this batch has one.
///
/// `RepetitionInspector` is the sole policy authority; `TurnToolGuard` only
/// converts its exact-request `Deny` into a terminal event and has no
/// independent counter or threshold.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn repetition_denial_abort(
    inspection_results: &[InspectionResult],
    permission_check_result: &PermissionCheckResult,
    turn_guard: &mut super::turn_guard::TurnToolGuard,
) -> Option<(TurnAbortCode, String)> {
    let (result, request) = inspection_results
        .iter()
        .filter(|result| {
            result.inspector_name == crate::tool_monitor::REPETITION_INSPECTOR_NAME
                && result.action == InspectionAction::Deny
        })
        .find_map(|result| {
            permission_check_result
                .denied
                .iter()
                .find(|request| request.id == result.tool_request_id)
                .map(|request| (result, request))
        })?;
    let code = turn_guard.enforce_denial(request)?;
    warn!(
        tool_request_id = %request.id,
        "repetition policy denied a tool signature; terminating this user turn"
    );
    Some((code, result.reason.clone()))
}

/// PAR-04: give every tool a cancel abandoned an explicit "cancelled" result.
///
/// The batch loop breaks the instant the token trips, abandoning every tool
/// that had not yet returned. Their response slots are still the empty
/// placeholders allocated up front, and the post-batch persistence loop writes
/// a `tool_use` for each request unconditionally — so without this backfill a
/// cancelled batch persists `tool_use` blocks with no matching `tool_result`,
/// which every provider rejects when the session is replayed.
///
/// A slot that already holds a response (the tools that finished before the
/// cancel) is left untouched, so no completed result is overwritten. Covers
/// frontend tools too: the persistence loop writes a `tool_use` for those as
/// well, and a cancel can land while one is still awaiting its client reply.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
async fn backfill_cancelled_tool_responses(
    frontend_requests: &[ToolRequest],
    remaining_requests: &[ToolRequest],
    request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
) {
    for request in frontend_requests.iter().chain(remaining_requests.iter()) {
        let Some(response_msg) = request_to_response_map.get(&request.id) else {
            continue;
        };
        let mut response = response_msg.lock().await;
        let already_answered = response.content.iter().any(|c| {
            matches!(
                c,
                MessageContent::ToolResponse(r)
                    if r.id == request.id
            )
        });
        if already_answered {
            continue;
        }
        *response = response.clone().with_tool_response_with_metadata(
            request.id.clone(),
            Ok(CallToolResult {
                content: vec![Content::text(
                    super::tool_execution::CANCELLED_MID_RUN_RESPONSE,
                )],
                structured_content: None,
                is_error: Some(true),
                meta: None,
            }),
            request.metadata.as_ref(),
        );
    }
}

/// BR-47: re-parse every file this tool batch wrote and return the ones whose
/// syntax is now broken.
///
/// A successful `text_editor` write is re-parsed with the developer analyzer's
/// tree-sitter grammars; any ERROR / MISSING nodes become agent-visible
/// corrective context, so the model fixes broken syntax in the same turn
/// instead of only discovering it if it happens to run tests. Runs off the
/// still-owned `post_tool_results`, before the PostToolUse hooks consume it.
///
/// `None` when the batch wrote nothing — which is NOT the same as an empty
/// `Some`: a batch that edited files and found them all clean still has to spend
/// a [`post_edit_reflection_text`] call, because that is what restores the
/// reflection budget.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn post_edit_file_diagnostics(
    post_tool_results: &[(String, Option<Value>, Option<String>)],
    remaining_requests: &[ToolRequest],
    working_dir: &std::path::Path,
    analyzer: &mut Option<biorouter_mcp::developer::analyze::CodeAnalyzer>,
) -> Option<Vec<crate::agents::post_edit_diagnostics::FileDiagnostics>> {
    use crate::agents::post_edit_diagnostics as ped;
    // (display path, resolved path) for each successful write.
    let mut edited: Vec<(String, std::path::PathBuf)> = Vec::new();
    for (request_id, _response_value, error_text) in post_tool_results {
        if error_text.is_some() {
            // The write itself failed; there is nothing valid on disk to parse.
            continue;
        }
        let Some(request) = remaining_requests.iter().find(|r| &r.id == request_id) else {
            continue;
        };
        let Some(resolved) = ped::edited_path_from_request(request, working_dir) else {
            continue;
        };
        // Show the model the path it actually sent, when readable.
        let display = request
            .tool_call
            .as_ref()
            .ok()
            .and_then(|tc| tc.arguments.as_ref())
            .and_then(|a| a.get("path").or_else(|| a.get("file_path")))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| resolved.display().to_string());
        edited.push((display, resolved));
    }
    if edited.is_empty() {
        return None;
    }
    let analyzer =
        analyzer.get_or_insert_with(biorouter_mcp::developer::analyze::CodeAnalyzer::new);
    let mut files: Vec<ped::FileDiagnostics> = Vec::new();
    // Dedup by resolved path: a file written twice in one batch is reported
    // once, on its final on-disk state.
    let mut seen = std::collections::HashSet::new();
    for (display, resolved) in edited {
        if !seen.insert(resolved.clone()) {
            continue;
        }
        let diags = analyzer.diagnose_file(&resolved);
        if diags.is_empty() {
            continue;
        }
        files.push(ped::FileDiagnostics {
            path: display,
            lines: diags.iter().map(|d| d.render()).collect(),
        });
    }
    Some(files)
}

/// BR-47: spend one post-edit reflection on `files`, returning the framed
/// diagnostics to inject (if any).
///
/// Bounded by a per-reply reflection counter so a file that never parses clean
/// cannot wedge the turn — the built-in twin of the BR-19 PostToolUse block cap.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn post_edit_reflection_text(
    session_id: &str,
    config: &crate::agents::post_edit_diagnostics::PostEditDiagnosticsConfig,
    files: &[crate::agents::post_edit_diagnostics::FileDiagnostics],
    reflections: &mut u32,
) -> Option<String> {
    use crate::agents::post_edit_diagnostics as ped;
    match ped::next_reflection(!files.is_empty(), *reflections, config.max_reflections) {
        ped::ReflectionOutcome::Reset => {
            // Every edited file parsed clean: a genuine fix (or a clean edit)
            // restores the budget.
            *reflections = 0;
            None
        }
        ped::ReflectionOutcome::Inject { next } => {
            *reflections = next;
            let total: usize = files.iter().map(|f| f.lines.len()).sum();
            tracing::info!(
                files = files.len(),
                diagnostics = total,
                reflection = *reflections,
                "BR-47: injecting post-edit syntax diagnostics"
            );
            loop_safety::emit(
                LoopSafetyEvent::new(LoopSafetyKind::PostEditDiagnostics)
                    .session(session_id)
                    .count(*reflections),
            );
            // Held, not pushed: it must land after the tool response for the
            // edit it describes.
            Some(ped::frame_post_edit_diagnostics(files))
        }
        ped::ReflectionOutcome::Capped => {
            // Deliver the result as-is so the turn is not wedged on a file that
            // never parses clean.
            tracing::info!(
                cap = config.max_reflections,
                "BR-47: post-edit diagnostics reflection cap reached; not injecting again this reply"
            );
            None
        }
    }
}

/// Fold one loop iteration's persisted messages into the running conversation
/// and update the signed Bedrock replay context.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
fn fold_iteration_into_conversation(
    conversation: &mut Conversation,
    signed_replay_context: &mut Option<Conversation>,
    conversation_with_moim: &Conversation,
    messages_to_add: Vec<Message>,
    did_retry_reset_this_iteration: bool,
    signed_replay_invalidated_this_iteration: bool,
) {
    let continues_signed_construction = continues_bedrock_assistant_construction(&messages_to_add);
    let replay_messages = canonicalize_signed_replay_suffix(&messages_to_add);
    let response_has_signed_reasoning = replay_messages.iter().any(has_signed_reasoning);
    if !did_retry_reset_this_iteration {
        conversation.extend(messages_to_add);
    }
    if signed_replay_invalidated_this_iteration
        || did_retry_reset_this_iteration
        || (!continues_signed_construction && response_has_signed_reasoning)
        || (signed_replay_context.is_some() && !continues_signed_construction)
    {
        if signed_replay_context.is_some() || response_has_signed_reasoning {
            let stripped = crate::conversation::without_bedrock_reasoning(conversation);
            *conversation = stripped;
        }
        *signed_replay_context = None;
    } else if continues_signed_construction
        && (signed_replay_context.is_some() || response_has_signed_reasoning)
    {
        signed_replay_context
            .get_or_insert_with(|| conversation_with_moim.clone())
            .extend(replay_messages);
    }
}

/// Fill the response slots of every tool call in a batch that a chat-mode turn
/// declines to run.
///
/// Split out of the `reply_internal` generator to keep its `poll` frame small —
/// see [`ToolBatchMaps`].
async fn skip_tool_requests_for_chat_mode(
    remaining_requests: &[ToolRequest],
    request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
) {
    for request in remaining_requests {
        if let Some(response_msg) = request_to_response_map.get(&request.id) {
            let mut response = response_msg.lock().await;
            *response = response.clone().with_tool_response_with_metadata(
                request.id.clone(),
                Ok(CallToolResult {
                    content: vec![Content::text(CHAT_MODE_TOOL_SKIPPED_RESPONSE)],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
                request.metadata.as_ref(),
            );
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ExtensionLoadResult {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct AgentConfig {
    pub session_manager: Arc<SessionManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub scheduler_service: Option<Arc<dyn SchedulerTrait>>,
    pub biorouter_mode: BioRouterMode,
}

impl AgentConfig {
    pub fn new(
        session_manager: Arc<SessionManager>,
        permission_manager: Arc<PermissionManager>,
        scheduler_service: Option<Arc<dyn SchedulerTrait>>,
        biorouter_mode: BioRouterMode,
    ) -> Self {
        Self {
            session_manager,
            permission_manager,
            scheduler_service,
            biorouter_mode,
        }
    }
}

/// What happened to a tool-permission decision handed to [`Agent::handle_confirmation`]
/// (BR-62).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationOutcome {
    /// A prompt with that request id was waiting; the decision reached the loop.
    Delivered,
    /// Nothing was waiting on that request id — a duplicate click, a decision for
    /// a prompt that already expired or was cancelled, or a stale client. The
    /// decision was dropped rather than applied to some other pending call.
    Unknown,
}

/// One queued mid-turn injection: the text plus who injected it (BR-71).
#[derive(Debug, Clone)]
pub struct QueuedInterrupt {
    pub text: String,
    pub provenance: Option<crate::conversation::message::MessageProvenance>,
}

/// Identity of one run of the agent's reply loop (#69).
///
/// Minted by the loop itself, **not** the server's `ActiveTurn.turn_id`: the two
/// counters live in different processes-worth of state and are deliberately given
/// different shapes (`agent-turn-N` here, `turn-N` there) so an id from one is
/// never mistaken for an id of the other. It exists so an accepted soft interrupt
/// can name the turn that took it — a 202 that cannot say *which* turn it landed
/// in is the ambiguity #69 is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TurnId(String);

impl TurnId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The next loop-run id in this process.
    pub(super) fn mint() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        Self(format!(
            "agent-turn-{}",
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TurnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<TurnId> for String {
    fn from(id: TurnId) -> String {
        id.0
    }
}

/// #69: the soft-interrupt queue owns whether it is accepting, so acceptance and
/// enqueue happen in one critical section. A prior `is_turn_active` check cannot
/// substitute: check-then-queue is two steps against state that changes underneath
/// them, and adding more checks only narrows the window.
pub(super) struct SoftInterrupts {
    /// The turn currently accepting, if any.
    turn: Option<TurnId>,
    /// Cleared by `close_and_drain` once the loop has committed to exiting.
    accepting: bool,
    queued: Vec<QueuedInterrupt>,
}

impl SoftInterrupts {
    fn new() -> Self {
        Self {
            turn: None,
            accepting: false,
            queued: Vec::new(),
        }
    }
}

/// Why [`Agent::try_queue_soft_interrupt`] would not take a steer (#69).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptRefused {
    /// No turn is accepting: either none is running, or the running one has closed.
    TurnEnded,
}

impl std::fmt::Display for InterruptRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TurnEnded => f.write_str("no turn is accepting interrupts for this session"),
        }
    }
}

/// Who is going to read the tool list [`Agent::list_tools_for`] builds.
///
/// Issue #56 Gate E hides a private extension's tool names, descriptions and
/// JSON schemas from a public MODEL, because schema text is content and it
/// reaches the model before any tool call exists for Gate C to refuse. It must
/// NOT hide them from the HUMAN who installed that extension: the permission
/// editors exist so that human can set a permission per tool, and a tool that is
/// not listed cannot be configured.
///
/// The two audiences are an enum rather than a `bool` so that the answer at each
/// call site reads as the privacy decision it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolAudience {
    /// The model's context. Gate E applies.
    Model,
    /// Settings → Extensions → tool permissions, and `biorouter configure`'s
    /// tool selector. Gate E does not apply — see Task 16's ⚠.
    PermissionEditor,
}

/// The outcome of the turn loop's take-and-close at its exit (#69).
///
/// `pub` for the same reason [`Agent::open_for_turn`] is: the route-level tests
/// have to be able to drive an agent through the turn-lifecycle transitions the
/// reply loop performs, without running a reply loop.
#[derive(Debug)]
pub enum Drained {
    /// Items were taken; the turn stays open and must loop again to consume them.
    Some(Vec<QueuedInterrupt>),
    /// Nothing queued; the queue is now closed and the loop may exit.
    Empty,
}

/// Issue #56 Gate B'. The classification of the session this agent is serving,
/// as last observed, plus the id it was observed for.
///
/// It exists because the transcript leaves the machine on three paths that
/// never enter [`Agent::reply`] and therefore never meet Gate B: session
/// auto-naming (`maybe_rename_session` → `maybe_update_name` →
/// `generate_session_name` → `complete_fast`), compaction summarisation
/// (`context_mgmt::compact_messages`) and the stall judge (`agents::stall`).
/// [`Agent::provider`] is where the assertion goes because it is the accessor
/// those paths reach the binding through — and an assertion needs the
/// classification without a session id to look it up by, because none of those
/// three call sites has one to give.
///
/// ⚠ **[`Agent::provider`] is not the only way to the binding, and the doc that
/// said so was wrong.** `SharedProvider` is an `Arc<Mutex<..>>` that is handed
/// out, and three production sites clone out of it directly:
///
/// * [`Agent::maybe_spawn_eager_compaction`] — the background half of a path
///   this cache DOES claim to cover. It cannot block on the lock, so it cannot
///   call the accessor; it asserts the same predicate inline instead. Covered.
/// * `permission::permission_inspector`'s smart-approve judge, which sends tool
///   names and arguments (not the transcript) to the model.
/// * `hooks::HooksManager`'s prompt hooks, which send the hook's own payload.
///
/// The last two hold a `SharedProvider` and no `&Agent`, so they have no
/// classification to consult and covering them is a signature change this task
/// does not own. Their exposure today is bounded — both run during or after a
/// turn Gate B has already repaired or refused — but neither is *asserted*, and
/// a later task must not cite this cache as though they were.
///
/// The cache is sound because `AgentManager` holds one `Arc<Agent>` per session
/// id, so "this agent" and "this session" are the same thing; it is re-synced
/// from the row at every `reply` entry, and reconciled by a successful bind in
/// between (see [`Agent::update_provider`]).
///
/// ⚠ It initialises to **`Private`**, not `Public`. It is read before the first
/// `reply` on a rehydrated agent, and the restrictive value is the safe default
/// there: a wrong `Private` costs one refused auto-naming, a wrong `Public`
/// costs a private transcript sent to a public model.
///
/// A plain `Mutex` rather than an atomic: it carries a `String` as well, it is
/// never held across an `await`, and every read is off the hot path.
///
/// # Known skews between this cache and the row
///
/// Both are narrow, both close at the next `reply`, and both are recorded here
/// because the repair-card task inherits them.
///
/// 1. **A cache reading `Public` against a `Private` row.**
///    [`Agent::update_provider`] stores `Public` on a successful public bind,
///    which is sound at the instant it runs (Gate A's `WHERE` admits a public
///    provider only against a public row). A ratchet that commits between that
///    `UPDATE` and the store then privatises the row without touching the
///    cache — the exact state
///    `a_ratchet_that_commits_after_a_legal_bind_lands_in_the_state_gate_b_owns`
///    constructs, since `raise_privacy` writes SQL and knows nothing about any
///    agent. Until the next `reply` re-syncs, Gate B' would admit a public
///    provider for an out-of-`reply` completion — and `maybe_rename_session` is
///    called from outside `reply` (`workspace/turn.rs`, `routes/apps.rs`). It
///    needs the bind task to be descheduled between two adjacent statements, so
///    it is genuinely narrow, but it is fail-OPEN and so is written down. The
///    alternative — leaving the cache at its `Private` default after a legal
///    public bind — is not a privacy control but a startup failure on every
///    fresh public session, because the CLI asserts `provider()` before its
///    first turn.
///
/// 2. **A cache reading `Private` for a session with no row.** The
///    `BindOutcome::NoSuchSession` arm deliberately does not refuse — an id
///    naming no row must never be reported to a user as "this chat is private"
///    — but it also stores nothing, so the cache stays at its fail-closed
///    default and a later `provider()` on that agent refuses. The two arms
///    disagree about the same non-existent session. Fail-CLOSED, and no traced
///    caller binds before its row exists (`subagent_tool` creates the child row
///    first; the CLI and the routes bind after `create_session`), so it is left
///    as the safer inconsistency rather than papered over.
#[derive(Debug)]
pub(super) struct CachedClassification {
    inner: std::sync::Mutex<(SessionClassification, Option<String>)>,
}

impl Default for CachedClassification {
    fn default() -> Self {
        Self {
            inner: std::sync::Mutex::new((SessionClassification::Private, None)),
        }
    }
}

impl CachedClassification {
    fn load(&self) -> SessionClassification {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0
    }

    fn store(&self, session_id: &str, classification: SessionClassification) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = (classification, Some(session_id.to_string()));
    }

    /// The session the cached classification was read for. Diagnostics only —
    /// it rides in the refusal so a handler can name the chat, and it is
    /// deliberately absent from the refusal's own message text (§14.4).
    fn session_id(&self) -> String {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1
            .clone()
            .unwrap_or_default()
    }
}

/// The main biorouter Agent
pub struct Agent {
    pub(super) provider: SharedProvider,
    pub config: AgentConfig,

    pub extension_manager: Arc<ExtensionManager>,
    pub(super) sub_workflows: Mutex<HashMap<String, SubWorkflow>>,
    /// Whether the generic `subagent` tool is offered at all.
    ///
    /// Default `true` (every existing caller). An Agent-Drafter app that declares
    /// worker profiles sets this `false`, because otherwise TWO delegation
    /// mechanisms are armed at once and the generic one is easier to reach: the
    /// `subagent` tool's description auto-lists the very worker names the author
    /// registered, and it takes a free-form `instructions` string. The model
    /// picked it every time — spec-006 declared the same four workers *twice*
    /// (`sub_agents` AND `agents`), and the declared profiles were dead config.
    /// A tool that is absent from the tool list cannot be called; prose competing
    /// with an available tool loses.
    pub(super) subagent_tool_enabled: AtomicBool,
    pub(super) final_output_tool: Arc<Mutex<Option<FinalOutputTool>>>,
    pub(super) frontend_tools: Mutex<HashMap<String, FrontendTool>>,
    pub(super) frontend_instructions: Mutex<Option<String>>,
    pub(super) prompt_manager: Mutex<PromptManager>,
    /// BR-62: tool-permission prompts still awaiting a decision, keyed by tool
    /// **request id**. One `oneshot` per prompt, registered *before* the
    /// confirmation message is yielded (so a fast client cannot answer into a
    /// void), replaces the single per-agent mpsc: a stale or duplicate
    /// `/action-required` POST can no longer resolve a *different* pending
    /// request, and [`Agent::handle_confirmation`] can tell its caller whether
    /// the id was still live — which is what makes the route idempotent.
    pub(super) pending_confirmations:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<PermissionConfirmation>>>>,
    pub(super) tool_result_tx: mpsc::Sender<(String, ToolResult<CallToolResult>)>,
    pub(super) tool_result_rx: ToolResultReceiver,

    pub(super) retry_manager: RetryManager,
    /// `Arc` so a bridged coding agent's tool call can be inspected by the very
    /// same inspector stack the reply loop uses. The manager is built once in
    /// `create_tool_inspection_manager` and only read afterwards, so sharing it
    /// costs nothing — and the alternative (giving the bridge a dispatch path with
    /// no inspectors) would mean a child agent's tool calls skipped PreToolUse
    /// hooks, the sensitive-operation checks and the permission mode entirely.
    pub(super) tool_inspection_manager: Arc<ToolInspectionManager>,
    pub(super) hooks_manager: Arc<crate::hooks::HooksManager>,
    /// Active `/goal` conditions per session (see [`crate::agents::goal`]).
    pub(super) goals: crate::agents::goal::GoalRegistry,
    /// Lazily-created scheduler for `/loop`/`/schedule` when no
    /// `scheduler_service` was injected (plain CLI/TUI sessions).
    pub(super) fallback_scheduler: tokio::sync::OnceCell<Arc<dyn SchedulerTrait>>,
    /// BRSDK encryption: per-app decrypted secrets, substituted into tool-call
    /// arguments at dispatch (`{{vault:NAME}}`). `None` for normal sessions.
    pub(super) vault: Mutex<Option<Arc<crate::agents::vault_refs::VaultRefs>>>,
    /// Soft-interrupt queue: user messages submitted mid-turn. Drained and
    /// injected at the next safe loop boundary in `reply_internal` instead of
    /// cancelling the turn (no lost work, no full context re-send). A plain
    /// `std::Mutex` so callers can push without awaiting the agent's async locks.
    ///
    /// #69: the guarded value also carries *which* turn is accepting and whether
    /// it still is, so a steer's acceptance and its enqueue are one critical
    /// section rather than a check the loop can invalidate in between.
    pub(super) soft_interrupts: Arc<std::sync::Mutex<SoftInterrupts>>,
    /// BR-43 shadow-git checkpoints: captures the work-tree at turn boundaries so
    /// `/rewind` can restore files/conversation. `None` when disabled (the
    /// default) or on the subagent/test paths. Gated by `BIOROUTER_CHECKPOINTS`.
    pub(super) checkpoints: Option<Arc<CheckpointManager>>,
    /// BR-12: sessions with a background eager-compaction task in flight. Keeps
    /// at most one summarizer running per session so two tasks never both try to
    /// swap in a compacted history. A plain `std::Mutex` — only held for the
    /// insert/remove, never across an await.
    pub(super) eager_compactions: Arc<std::sync::Mutex<HashSet<String>>>,
    /// Per-session set of skill names whose full body has already been inlined
    /// into an earlier turn's `<explicit-resource-context>` (BR-8). A skill's
    /// body is inlined in full the first turn it is selected, then replaced by a
    /// short pointer on later turns so a skill-heavy session doesn't re-pay the
    /// multi-KB body cost every turn. Keyed by session id.
    pub(super) injected_skills: Mutex<HashMap<String, std::collections::HashSet<String>>>,
    /// BR-31 no-progress detector. The same config the `RepetitionInspector`
    /// carries: the inspector owns the hard stop (it can only block a *future*
    /// call), while the reply loop owns the escalating nudges, which it emits at
    /// the result-collection seam as soon as the failing result lands.
    pub(super) failure_loop: FailureLoopConfig,
    /// BR-18: tool-name → risk grade, derived from each tool's MCP annotations
    /// and refreshed in `prepare_tools_and_prompt` from the exact tool list the
    /// model is handed this turn (so platform/frontend tools and freshly-enabled
    /// extensions are graded too). Shared with the `PermissionInspector`, which
    /// reads it to auto-approve read-only calls in `SmartApprove`. Before this,
    /// its predecessor sets were constructed empty and never populated, so the
    /// read-only short-circuit was unreachable.
    pub(super) tool_risks: Arc<ToolRiskRegistry>,
    /// BR-63: per-session sticky reasoning effort, set by `/effort`. A per-turn
    /// effort on the `SessionConfig` (the GUI composer toggle) wins over it.
    pub(super) efforts: crate::agents::effort::EffortRegistry,
    /// BR-56: caches the normalized prefix of the transcript so `fix_conversation`
    /// only re-runs over the messages appended since the last call — the agent
    /// normalizes at least once per reply and once per provider call, and a full
    /// pass is O(history). A prefix is only reused while every message in it
    /// fingerprints the same, so a rewritten history (compaction, `HistoryReplaced`,
    /// a session reload, a different session on a shared agent) simply misses and
    /// falls back to a full normalization.
    pub(super) normalizer: crate::conversation::SharedNormalizer,
    /// Issue #56 Gate B'. See [`CachedClassification`].
    pub(super) cached_classification: CachedClassification,
}

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    ModelChange {
        model: String,
        mode: String,
    },
    HistoryReplaced(Conversation),
    /// A tool call the model has started emitting, announced as soon as its
    /// **name** is known — typically seconds before its arguments finish
    /// generating.
    ///
    /// Purely advisory, purely for the UI. It is a distinct `AgentEvent`
    /// variant rather than a `Message` on purpose: `categorize_tools`,
    /// `num_tool_requests` and the dispatch loop only ever walk `Message`
    /// contents, so a pending call is *structurally* incapable of being
    /// executed with truncated arguments, gated, persisted, or replayed.
    /// Never turn this into a `MessageContent`. See [`PendingToolCall`].
    ToolCallPending(crate::providers::base::PendingToolCall),
    /// BR-52: the session's token counters as of the last turn/compaction
    /// boundary, emitted by the agent right after it wrote them.
    ///
    /// Token accounting only changes at those boundaries — never mid-stream —
    /// so a consumer can cache this and attach it to every event it forwards.
    /// The server used to re-read the counters from SQLite on *every* streamed
    /// chunk, which was pure redundant disk work on the hottest path.
    TokenUsage(TokenState),
    /// The turn ended **without doing its work**.
    ///
    /// A provider failure used to be yielded only as an assistant `Message`
    /// ("Ran into this error: …") after which the stream ended normally — so a
    /// caller could not distinguish a 403 from a completed turn without
    /// regex-matching English prose. `biorouter run` exited 0 on an auth failure
    /// and telemetry recorded it as a success.
    ///
    /// The human-readable `Message` is still emitted first (the desktop UX shows
    /// it); this event is the machine-checkable companion, and it is always
    /// terminal. See [`crate::agents::turn_abort`].
    TurnAborted {
        code: TurnAbortCode,
        message: String,
    },
    /// #59: the ids the turn's rows were actually persisted under.
    ///
    /// A client that watched a whole turn go by used to end it knowing **none**
    /// of the ids the store holds, which is why `expectedMessageIds` on
    /// `POST /sessions/{id}/edit_message` — the ids of every message the
    /// client's view holds, refused with 409 when the store holds one the view
    /// does not name — could not be satisfied by any client and had to be made
    /// optional.
    ///
    /// Two things stood in the way, and both are answered here rather than by
    /// re-shaping the stream:
    ///
    /// * a message can be **yielded under a different id than it is stored
    ///   under** — `add_message` re-mints on a uid collision, and the streamed
    ///   assistant reply becomes up to three stored rows (thinking / tool
    ///   request / response), only the first of which keeps the reply's id;
    /// * a message can be **stored without ever being yielded** — the BR-47
    ///   post-edit diagnostics, the loop-guard / stall / budget nudges and the
    ///   hook-context injections are model-visible plumbing the user must not
    ///   see in the transcript.
    ///
    /// So every persist site publishes what it stored. Emitted immediately
    /// after the rows are durable, so a consumer that stops reading mid-turn
    /// never holds an id for a row that was not written.
    MessagesPersisted(Vec<PersistedMessage>),
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

pub enum ToolStreamItem<T> {
    Message(ServerNotification),
    Result(T),
}

pub type ToolStream =
    Pin<Box<dyn Stream<Item = ToolStreamItem<ToolResult<CallToolResult>>> + Send>>;

// tool_stream combines a stream of ServerNotifications with a future representing the
// final result of the tool call. MCP notifications are not request-scoped, but
// this lets us capture all notifications emitted during the tool call for
// simpler consumption
pub fn tool_stream<S, F>(rx: S, done: F) -> ToolStream
where
    S: Stream<Item = ServerNotification> + Send + Unpin + 'static,
    F: Future<Output = ToolResult<CallToolResult>> + Send + 'static,
{
    Box::pin(async_stream::stream! {
        tokio::pin!(done);
        let mut rx = rx;

        loop {
            tokio::select! {
                Some(msg) = rx.next() => {
                    yield ToolStreamItem::Message(msg);
                }
                r = &mut done => {
                    yield ToolStreamItem::Result(r);
                    break;
                }
            }
        }
    })
}

/// What woke the tool-batch loop (#40).
#[derive(Debug)]
pub(crate) enum BatchWake<T> {
    /// The turn's cancel token tripped.
    Cancelled,
    /// An elicitation request was queued on the [`ActionRequiredManager`].
    /// The tool call that raised it is itself parked *inside* the batch —
    /// blocked in `create_elicitation` until the request is answered or
    /// cancelled — so this MUST be able to preempt `combined.next()`: a
    /// consumer that only drained the request queue after a tool item
    /// yielded could never surface the request, and a headless auto-cancel
    /// had to wait out the full 300 s elicitation timeout.
    ElicitationReady,
    /// The next tool-stream item (or `None`: the batch is drained).
    Item(Option<T>),
}

/// Race the batch's next tool item against the turn's cancel token and the
/// arrival of an elicitation request **for this session**. Biased so a cancel
/// always wins and an elicitation beats a tool item. Every branch is
/// cancel-safe: `cancelled()` and `Notify::notified` re-arm losslessly, and
/// `StreamExt::next` never drops a resolved item on a lost race.
///
/// The wake is scoped by `session_id`: the manager is process-global, and an
/// unscoped wake let ANY concurrent session's batch loop win the race, drain
/// the request, and persist the elicitation prompt under its own session id
/// (#40). A request scoped to another session neither wakes nor is drained
/// by this loop.
pub(crate) async fn next_batch_wake<T, S>(
    cancel_token: &Option<CancellationToken>,
    combined: &mut S,
    session_id: &str,
) -> BatchWake<T>
where
    S: Stream<Item = T> + Unpin,
{
    tokio::select! {
        biased;
        _ = async {
            match cancel_token.as_ref() {
                Some(token) => token.cancelled().await,
                None => std::future::pending::<()>().await,
            }
        } => BatchWake::Cancelled,
        _ = ActionRequiredManager::global().request_arrived(session_id) => BatchWake::ElicitationReady,
        item = combined.next() => BatchWake::Item(item),
    }
}

/// BR-12: RAII marker that a session has a background eager-compaction task in
/// flight. Removed from the agent's `eager_compactions` set on drop (task ends,
/// panics, or the runtime shuts down), so a later turn can spawn again.
struct EagerCompactionGuard {
    session_id: String,
    in_flight: Arc<std::sync::Mutex<HashSet<String>>>,
}

impl Drop for EagerCompactionGuard {
    fn drop(&mut self) {
        self.in_flight
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.session_id);
    }
}

/// Fire a Pre/PostCompact hook without an `Agent` receiver. Split out of
/// [`Agent::fire_compaction_hook`] so the BR-12 background eager-compaction task
/// (which holds only a cloned `Arc<HooksManager>`, not `&self`) can fire the
/// same hooks.
pub(super) fn fire_compaction_hook_on(
    hooks_manager: &Arc<crate::hooks::HooksManager>,
    event: crate::hooks::HookEvent,
    session_id: &str,
    working_dir: &std::path::Path,
    trigger: &str,
    reason: Option<&str>,
) {
    let mut payload =
        crate::hooks::HookPayload::new(event, session_id, working_dir.to_string_lossy());
    payload.trigger = Some(trigger.to_string());
    payload.reason = reason.map(str::to_string);
    hooks_manager.fire(
        event,
        Some(trigger.to_string()),
        payload,
        working_dir.to_path_buf(),
    );
}

/// Rendezvous points inside the agent loop that a test needs to stop the world
/// at, because the property under test is an ORDERING and no amount of
/// `tokio::time::sleep` makes an ordering deterministic.
///
/// Issue #56. The first of them, [`hold_dispatch_queue`], parks a dispatched
/// tool call exactly where a real queued call sits: after
/// `Agent::dispatch_tool_call` has returned its future — so the capability has
/// already been sampled — and before anything drives it. Tasks 12 and 14B/14D
/// add their own rendezvous to this same module.
///
/// ⚠ The rendezvous is process-global, so it is KEYED rather than first-come.
/// An unkeyed slot degrades the test that uses it to a SILENT PASS: `cargo test`
/// runs `--lib` tests concurrently in one process, so any other test driving an
/// `Agent::dispatch_tool_call` future would take the rendezvous meant for the
/// armer, whose own call then sails through un-parked and completes before the
/// provider swap it exists to order against — and still asserts green, because
/// the capability it read was the one it started with either way. The ordering
/// simply stops being tested. Keying on `(session id, tool name)` makes the
/// caught call the intended call by construction, and holding a LIST of arms
/// means two tests can be armed at once without clobbering each other.
#[cfg(test)]
pub(crate) mod seams {
    use crate::providers::base::Provider;
    use tokio::sync::oneshot;

    /// One armed rendezvous: the caller session id and tool name it is waiting
    /// for, and the channel the matching call announces itself on.
    type ArmedDispatch = (String, String, oneshot::Sender<oneshot::Sender<()>>);

    /// The rendezvous a test armed but no dispatch has matched yet.
    static ARMED: std::sync::Mutex<Vec<ArmedDispatch>> = std::sync::Mutex::new(Vec::new());

    /// Arm the rendezvous for ONE specific dispatch — the next call of
    /// `tool_name` made from `session_id`. Await the returned receiver to learn
    /// that call has arrived and to get the sender that releases it.
    ///
    /// `tool_name` is the wire name the agent dispatches, i.e. the prefixed
    /// `"<extension>__<tool>"` form, because that is what reaches the hold
    /// point. A key that matches nothing parks the caller forever, so a test
    /// should await the receiver under a `tokio::time::timeout` and fail
    /// loudly rather than hang.
    pub fn hold_dispatch_queue(
        session_id: &str,
        tool_name: &str,
    ) -> oneshot::Receiver<oneshot::Sender<()>> {
        let (arrived_tx, arrived_rx) = oneshot::channel();
        ARMED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((session_id.to_string(), tool_name.to_string(), arrived_tx));
        arrived_rx
    }

    /// The hold point itself. A no-op — one uncontended mutex lock — for every
    /// dispatch except one a test armed for by name, and the arm is consumed as
    /// it fires so only the first matching dispatch is caught.
    pub(super) async fn dispatch_queue_hold(session_id: &str, tool_name: &str) {
        let armed = {
            let mut slots = ARMED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            slots
                .iter()
                .position(|(s, t, _)| s == session_id && t == tool_name)
                .map(|i| slots.remove(i).2)
        };
        if let Some(arrived_tx) = armed {
            let (release_tx, release_rx) = oneshot::channel();
            if arrived_tx.send(release_tx).is_ok() {
                let _ = release_rx.await;
            }
        }
    }

    // ─── Issue #56, Gate A: the two rendezvous on the provider-bind path ────
    //
    // `arm_*` returns a [`Rendezvous`] that fires when the bind path reaches
    // it, carrying the sender that releases it — so a test can run a whole
    // ratchet *inside* the window instead of hoping a `tokio::spawn` lands
    // there. Two channels and not a `Barrier`: a 2-party `Barrier::wait`
    // releases both sides at the rendezvous, which is the one thing this must
    // not do.
    //
    // ⚠ THE TWO SEAMS SIT IN DIFFERENT FUNCTIONS, ON PURPOSE.
    // `before_bind_write` is inside `SessionStorage::bind_provider_if_allowed`
    // (`session_manager.rs`), between any read that function performs and the
    // statement that writes — hence `pub(crate)`, and hence the name: it is
    // *before the WRITE*, not merely before the call. `after_bind_before_swap`
    // is in `Agent::update_provider`, between the persist and the in-memory
    // swap. A seam at the call site instead of inside the helper cannot tell a
    // conditional `UPDATE` from a `SELECT` followed by an unconditional one,
    // which is the exact implementation Gate A exists to reject.
    //
    // ⚠ THE RENDEZVOUS IS PROCESS-GLOBAL, so — exactly as for
    // `hold_dispatch_queue` above — it must not be first-come, and "first-come"
    // has TWO forms here. Neither is hypothetical: the two forced-interleaving
    // tests and the 200-iteration fuzz loop are `#[tokio::test]`s in one
    // binary, which cargo runs on parallel threads with nothing serialising
    // them. There is no session id at the write to key on (the seam call has to
    // stay argument-free so the structural gate can pin its position), so
    // identity travels with the CALLING TASK instead, in a task-local:
    //
    //   1. A bind nobody armed. Only a future wrapped in [`armed`] is eligible
    //      at all. Without that, the fuzz loop's 400 bind arrivals over several
    //      seconds would routinely take the arm meant for a forced test, whose
    //      own bind then races the ratchet it was supposed to be ordered
    //      against.
    //   2. A bind armed for the OTHER seam. `after_bind_before_swap`'s test
    //      still traverses `before_bind_write` on its way — EVERY bind does, it
    //      is inside the storage helper — so a token that said only "this task
    //      is armed" would let that bind consume `before_bind_write`'s arm. The
    //      theft is invisible exactly where it matters: the robbed test's
    //      `arrived()` resolves from the wrong task, its own bind is never
    //      parked, and on a lucky schedule it passes having forced nothing —
    //      the silent pass a seam exists to make impossible. So the token names
    //      its seam and [`park`] compares before consuming.
    //
    // Identity is per-ARM rather than per-seam: `arm_*` mints a fresh id, so
    // two tests arming the same seam coexist instead of clobbering each other's
    // sender. An arm nobody consumes is inert — its token exists nowhere but
    // inside the one future it was minted for.

    /// Which of the two bind rendezvous a token authorizes.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Seam {
        BeforeBindWrite,
        AfterBindBeforeSwap,
    }

    /// What one `arm_*` call hands out: permission for one future to be caught
    /// once, at one named seam. `Copy` because it rides in a task-local.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) struct ArmToken {
        seam: Seam,
        id: u64,
    }

    type ArmedBind = (ArmToken, oneshot::Sender<oneshot::Sender<()>>);

    /// Arms placed but not yet consumed.
    static ARMED_BINDS: std::sync::Mutex<Vec<ArmedBind>> = std::sync::Mutex::new(Vec::new());
    static NEXT_ARM_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    tokio::task_local! {
        /// Set for the duration of a future wrapped in [`armed`]: the ONE arm
        /// that future may consume, at the ONE seam that arm names.
        static ARMED_TASK: ArmToken;
    }

    /// One armed rendezvous, handed back by `arm_before_bind_write` /
    /// `arm_after_bind_before_swap`.
    pub(crate) struct Rendezvous {
        token: ArmToken,
        arrived: oneshot::Receiver<oneshot::Sender<()>>,
    }

    impl Rendezvous {
        /// The token to hand to [`armed`]. It comes from the rendezvous itself,
        /// so a test cannot arm one seam and authorize its bind for the other.
        pub(crate) fn token(&self) -> ArmToken {
            self.token
        }

        /// Await arrival at this seam; yields the sender that releases it.
        ///
        /// Bounded, because the likeliest failure is that nothing ever arrives
        /// — a seam call that drifted out of the function, or a bind armed for
        /// the other one — and a test that hangs is a CI timeout with no
        /// message instead of a failure with one.
        pub(crate) async fn arrived(self) -> oneshot::Sender<()> {
            const WAIT: std::time::Duration = std::time::Duration::from_secs(10);
            tokio::time::timeout(WAIT, self.arrived)
                .await
                .expect(
                    "nothing reached this seam within 10s: the seam call drifted out of the \
                     function, or the bind was armed for the other seam",
                )
                .expect("the arm was dropped without firing")
        }

        /// Whether anything has announced itself here yet. The NEGATIVE is what
        /// the cross-seam test asserts, so only a real arrival counts — an arm
        /// still sitting unconsumed reads `false`.
        pub(crate) fn has_fired(&mut self) -> bool {
            // `is_ok()` and not "not Empty": a sender dropped without firing is
            // `Err(Closed)`, and that is an arm nobody took.
            self.arrived.try_recv().is_ok()
        }
    }

    /// Mark `fut` as the ONE call that may consume `token`'s arm.
    ///
    /// Every other `update_provider` in the process walks through both seams
    /// with one uncontended `try_with` and no await.
    pub(crate) fn armed<F: std::future::Future>(
        token: ArmToken,
        fut: F,
    ) -> impl std::future::Future<Output = F::Output> {
        ARMED_TASK.scope(token, fut)
    }

    fn arm(seam: Seam) -> Rendezvous {
        let (tx, rx) = oneshot::channel();
        let token = ArmToken {
            seam,
            id: NEXT_ARM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };
        ARMED_BINDS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((token, tx));
        Rendezvous { token, arrived: rx }
    }

    async fn park(seam: Seam) {
        // Unarmed, or armed for the other seam — one `try_with`, no await, no
        // lock. That is every bind in the process except the one under test.
        let token = match ARMED_TASK.try_with(|token| *token) {
            Ok(token) if token.seam == seam => token,
            _ => return,
        };
        // The guard is dropped at the end of this statement, before the await:
        // holding a std::sync::MutexGuard across an await point is the classic
        // way to turn a test seam into a deadlock.
        let armed = {
            let mut arms = ARMED_BINDS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            arms.iter()
                .position(|(armed, _)| *armed == token)
                .map(|i| arms.remove(i).1)
        };
        if let Some(reached) = armed {
            let (release_tx, release_rx) = oneshot::channel();
            if reached.send(release_tx).is_ok() {
                let _ = release_rx.await;
            }
        }
    }

    pub(crate) fn arm_before_bind_write() -> Rendezvous {
        arm(Seam::BeforeBindWrite)
    }

    pub(crate) fn arm_after_bind_before_swap() -> Rendezvous {
        arm(Seam::AfterBindBeforeSwap)
    }

    /// Called from `session_manager.rs`, hence `pub(crate)`.
    pub(crate) async fn before_bind_write() {
        park(Seam::BeforeBindWrite).await
    }

    pub(super) async fn after_bind_before_swap() {
        park(Seam::AfterBindBeforeSwap).await
    }

    // ─── Issue #56, Gate B: the provider a repairing rebind constructs ───────
    //
    // Gate B's repair arm exists to build the provider the session ROW names,
    // which in production means `providers::create` — a factory that reads the
    // user's config file and their OS keyring, and whose products talk to real
    // hosts. A unit test cannot go through it in either direction:
    // `create("versa_azure", ..)` needs institutional credentials this machine
    // may not have (and asking for them can raise a Keychain prompt), while
    // `create("ollama", ..)` succeeds *offline* and then points the turn at
    // whatever happens to be listening on localhost:11434.
    //
    // So the construction step — and only that step — is overridable in test
    // builds. Keyed by `(session id, provider name)`, not by name alone: these
    // tests are `#[tokio::test]`s in one binary on parallel threads, and a
    // name-only key would let one test's rebind answer another's.
    type RebindOverride = (String, String, std::sync::Arc<dyn Provider>);
    static REBIND_OVERRIDES: std::sync::Mutex<Vec<RebindOverride>> =
        std::sync::Mutex::new(Vec::new());

    /// Register the provider `Agent::rebind_from_row` must hand back when the
    /// row for `session_id` names `provider_name`.
    pub(crate) fn override_rebind_provider(
        session_id: &str,
        provider_name: &str,
        provider: std::sync::Arc<dyn Provider>,
    ) {
        REBIND_OVERRIDES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((session_id.to_string(), provider_name.to_string(), provider));
    }

    /// The registered override, if any. Not consumed: a session's rebind can
    /// legitimately happen on more than one turn.
    pub(super) fn rebind_override(
        session_id: &str,
        provider_name: &str,
    ) -> Option<std::sync::Arc<dyn Provider>> {
        REBIND_OVERRIDES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(s, p, _)| s == session_id && p == provider_name)
            .map(|(_, _, provider)| std::sync::Arc::clone(provider))
    }
}

impl Agent {
    pub fn new() -> Self {
        Self::with_config(AgentConfig::new(
            Arc::new(SessionManager::instance()),
            PermissionManager::instance(),
            None,
            Config::global()
                .get_biorouter_mode()
                .unwrap_or(BioRouterMode::Auto),
        ))
    }

    pub fn with_config(config: AgentConfig) -> Self {
        // Create channels with buffer size 32 (adjust if needed)
        let (tool_tx, tool_rx) = mpsc::channel(32);
        let provider = Arc::new(Mutex::new(None));

        let session_manager = Arc::clone(&config.session_manager);
        let permission_manager = Arc::clone(&config.permission_manager);
        // Load the managed/enterprise policy once at startup and share it across
        // the hooks manager and the tool inspectors (BR-65).
        let managed = ManagedPolicy::load();
        let hooks_manager = Arc::new(crate::hooks::HooksManager::new_with_managed(
            provider.clone(),
            Arc::clone(&managed),
        ));
        // BR-43: build the checkpoint manager only when enabled, so the disabled
        // default path never touches disk. Reads `BIOROUTER_CHECKPOINTS` / caps.
        let checkpoint_cfg = CheckpointConfig::from_env();
        let checkpoints = checkpoint_cfg.enabled.then(|| {
            Arc::new(CheckpointManager::new(
                crate::config::paths::Paths::data_dir(),
                Arc::clone(&config.session_manager),
                checkpoint_cfg,
            ))
        });
        // BR-18: one risk table, shared by the agent (which refreshes it from the
        // per-turn tool list) and the permission inspector (which reads it).
        let tool_risks = Arc::new(ToolRiskRegistry::new());
        Self {
            provider: provider.clone(),
            config,
            extension_manager: Arc::new(ExtensionManager::new(provider.clone(), session_manager)),
            sub_workflows: Mutex::new(HashMap::new()),
            subagent_tool_enabled: AtomicBool::new(true),
            final_output_tool: Arc::new(Mutex::new(None)),
            frontend_tools: Mutex::new(HashMap::new()),
            frontend_instructions: Mutex::new(None),
            prompt_manager: Mutex::new(PromptManager::new()),
            pending_confirmations: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tool_result_tx: tool_tx,
            tool_result_rx: Arc::new(Mutex::new(tool_rx)),
            retry_manager: RetryManager::new(),
            tool_inspection_manager: Arc::new(Self::create_tool_inspection_manager(
                permission_manager,
                Arc::clone(&hooks_manager),
                Arc::clone(&managed),
                Arc::clone(&tool_risks),
                provider.clone(),
            )),
            hooks_manager,
            goals: Default::default(),
            fallback_scheduler: tokio::sync::OnceCell::new(),
            vault: Mutex::new(None),
            soft_interrupts: Arc::new(std::sync::Mutex::new(SoftInterrupts::new())),
            checkpoints,
            eager_compactions: Arc::new(std::sync::Mutex::new(HashSet::new())),
            injected_skills: Mutex::new(HashMap::new()),
            failure_loop: Self::failure_loop_config(Config::global()),
            tool_risks,
            efforts: Default::default(),
            normalizer: Default::default(),
            cached_classification: Default::default(),
        }
    }

    /// Install the per-app secret vault (BRSDK encryption). Decrypted secrets are
    /// substituted into tool-call arguments at dispatch — after the model has
    /// produced the call — so plaintext never enters the model's context.
    pub async fn set_vault(&self, refs: Arc<crate::agents::vault_refs::VaultRefs>) {
        *self.vault.lock().await = Some(refs);
    }

    /// Resolve `{{vault:NAME}}` placeholders in a tool call's arguments using the
    /// installed vault (no-op when none is set). Called ONLY on the leaf
    /// MCP-dispatch path in [`Self::dispatch_tool_call`] — never for the subagent,
    /// frontend, final_output, or schedule branches, whose arguments would carry
    /// the plaintext back to an LLM/browser/store. (Residual: a tool that echoes
    /// its arguments in its *result* can still surface the secret on the next turn
    /// — that's outside the request-side substitution's control.)
    pub(super) async fn apply_vault(&self, tool_call: &mut CallToolRequestParams) {
        let vault = { self.vault.lock().await.clone() };
        if let Some(vault) = vault {
            if let Some(args) = tool_call.arguments.as_mut() {
                vault.resolve_args(args);
            }
        }
    }

    /// The soft-interrupt queue's guard, recovered past a poisoning. The guarded
    /// value is only ever pushed to, taken from, or re-flagged, so no invariant
    /// can be mid-update when a panic poisons it — dropping an injection because
    /// some unrelated task panicked would be strictly worse.
    fn lock_interrupts(&self) -> std::sync::MutexGuard<'_, SoftInterrupts> {
        self.soft_interrupts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Open the queue for a new turn (#69). Any straggler from a previous turn is
    /// dropped and logged rather than silently injected — that is the #69 bug's
    /// own shape.
    ///
    /// `pub` because the *route-level* tests have to be able to put an agent into
    /// the accepting state without running a whole reply loop; the only production
    /// caller is the reply loop itself.
    pub fn open_for_turn(&self, turn: TurnId) {
        let mut q = self.lock_interrupts();
        if !q.queued.is_empty() {
            warn!(
                count = q.queued.len(),
                "dropping interrupts left by a previous turn; they were never accepted"
            );
            q.queued.clear();
        }
        q.turn = Some(turn);
        q.accepting = true;
    }

    /// Re-open the queue after the loop decided *not* to exit at a point where it
    /// had already closed (a blocked Stop hook, a red done-gate, a self-critique
    /// revision). Without this the queue would stay shut for the rest of a turn
    /// that is demonstrably still running, and every steer would be refused.
    /// Keeps the queue's contents — unlike [`Agent::open_for_turn`], nothing here
    /// starts a new turn.
    pub(super) fn reopen_for_more_work(&self) {
        let mut q = self.lock_interrupts();
        if q.turn.is_some() {
            q.accepting = true;
        }
    }

    /// Accept a steer if and only if a turn is still accepting (#69). Returns the
    /// turn it landed in, so the caller can be told what its 202 actually promised.
    pub fn try_queue_soft_interrupt(
        &self,
        text: String,
        provenance: Option<crate::conversation::message::MessageProvenance>,
    ) -> Result<TurnId, InterruptRefused> {
        let mut q = self.lock_interrupts();
        if !q.accepting {
            return Err(InterruptRefused::TurnEnded);
        }
        let turn = q.turn.clone().ok_or(InterruptRefused::TurnEnded)?;
        q.queued.push(QueuedInterrupt { text, provenance });
        Ok(turn)
    }

    /// Queue a user message to be injected into the running turn at the next safe
    /// loop boundary (soft interrupt). Cheap + lock-light: callable from a server
    /// route or the CLI while a turn is streaming, without cancelling it.
    ///
    /// **Unguarded** — see [`Agent::queue_soft_interrupt_with_provenance`]. New
    /// callers that report success to anyone want
    /// [`Agent::try_queue_soft_interrupt`] instead.
    pub fn queue_soft_interrupt(&self, text: String) {
        self.queue_soft_interrupt_with_provenance(text, None);
    }

    /// BR-71: queue a mid-turn injection stamped with its origin. Used by the
    /// subagent-tab steer path.
    ///
    /// **Unguarded**: this pushes whether or not a turn is accepting, and returns
    /// `()`, so a caller can never learn that its text will not be consumed. That
    /// is why #69 added [`Agent::try_queue_soft_interrupt`], which is what the
    /// `/interrupt` route and `workspace_send_prompt mode:"steer"` use. This entry
    /// point survives for in-loop producers that already know a turn is running.
    /// Anything it leaves behind is dropped (with a warning) by the next
    /// [`Agent::open_for_turn`] rather than injected into an unrelated turn.
    pub fn queue_soft_interrupt_with_provenance(
        &self,
        text: String,
        provenance: Option<crate::conversation::message::MessageProvenance>,
    ) {
        let mut q = self.lock_interrupts();
        q.queued.push(QueuedInterrupt { text, provenance });
    }

    /// Drain queued soft-interrupt messages (FIFO). Returns empty when none.
    /// Leaves the queue *open*: this is the mid-turn drain at the top of a loop
    /// step, not the turn's exit (which is [`Agent::close_and_drain`]).
    pub(super) fn drain_soft_interrupts(&self) -> Vec<QueuedInterrupt> {
        let mut q = self.lock_interrupts();
        std::mem::take(&mut q.queued)
    }

    /// Take everything and, only if there was nothing, close in the same critical
    /// section (#69). That is what makes the exit atomic: after `Drained::Empty`
    /// no further interrupt can be accepted, so nothing can arrive between the
    /// check and the exit.
    pub fn close_and_drain(&self) -> Drained {
        let mut q = self.lock_interrupts();
        let taken = std::mem::take(&mut q.queued);
        if taken.is_empty() {
            q.accepting = false;
            Drained::Empty
        } else {
            Drained::Some(taken)
        }
    }

    /// Put items taken by [`Agent::close_and_drain`] back at the head of the
    /// queue, for the same turn's next loop step to consume in order. Anything
    /// accepted in between (the queue is still open) keeps its place behind them.
    pub(super) fn requeue_for_this_turn(&self, pending: Vec<QueuedInterrupt>) {
        let mut q = self.lock_interrupts();
        let mut restored = pending;
        restored.append(&mut q.queued);
        q.queued = restored;
    }

    /// Whether any soft interrupt is still waiting to be injected. Observation
    /// only — the turn's exit uses [`Agent::close_and_drain`], because a *check*
    /// here followed by an exit there is the two-step #69 removed.
    pub fn has_soft_interrupts(&self) -> bool {
        !self.lock_interrupts().queued.is_empty()
    }

    /// The hooks manager driving user-configured lifecycle hooks.
    pub fn hooks_manager(&self) -> Arc<crate::hooks::HooksManager> {
        Arc::clone(&self.hooks_manager)
    }

    /// The BR-43 checkpoint manager, when checkpoints are enabled.
    pub fn checkpoints(&self) -> Option<Arc<CheckpointManager>> {
        self.checkpoints.clone()
    }

    /// BR-43: snapshot the work-tree at a turn boundary (no-op when disabled).
    /// Best-effort — a checkpoint failure must never break the reply. `anchor_ts`
    /// is the `created` timestamp of the user message that opened this turn.
    pub(super) async fn maybe_checkpoint(
        &self,
        session_id: &str,
        working_dir: &std::path::Path,
        anchor_ts: i64,
        kind: CheckpointKind,
    ) {
        let Some(cp) = self.checkpoints.as_ref() else {
            return;
        };
        if let Err(e) = cp.snapshot(session_id, working_dir, anchor_ts, kind).await {
            warn!("BR-43 checkpoint snapshot failed (non-fatal): {e}");
        }
    }

    /// Fire a PreCompact/PostCompact hook (observe-only, fire-and-forget).
    pub(super) fn fire_compaction_hook(
        &self,
        event: crate::hooks::HookEvent,
        session_id: &str,
        working_dir: &std::path::Path,
        trigger: &str,
        reason: Option<&str>,
    ) {
        fire_compaction_hook_on(
            &self.hooks_manager,
            event,
            session_id,
            working_dir,
            trigger,
            reason,
        );
    }

    /// Create a tool inspection manager with default inspectors
    fn create_tool_inspection_manager(
        permission_manager: Arc<PermissionManager>,
        hooks_manager: Arc<crate::hooks::HooksManager>,
        managed: Arc<ManagedPolicy>,
        tool_risks: Arc<ToolRiskRegistry>,
        provider: SharedProvider,
    ) -> ToolInspectionManager {
        let mut tool_inspection_manager = ToolInspectionManager::new();

        // Managed/enterprise policy inspector (highest priority - runs first).
        // Its Deny/Ask verdicts ride the escalation-only merge and win over
        // every later inspector, including Auto mode's blanket Allow. Inert
        // (skipped) when no trusted managed file is present (BR-65).
        tool_inspection_manager
            .add_inspector(Box::new(ManagedPolicyInspector::new(Arc::clone(&managed))));

        // Add security inspector (runs after managed)
        tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));

        // Directive 2: in Fully-Automatic (Auto) mode, escalate the small set of
        // extremely-sensitive file operations (writes/deletes under system dirs,
        // SSH keys, keychains, launchd, browser credential stores, …) to the
        // standard approval flow. Inert in every other mode (see
        // `security::sensitive_ops`), so non-Auto behaviour is unchanged. It
        // emits `RequireApproval`, never `Deny`, so the catastrophic denylist and
        // command policy engine remain the non-bypassable floor.
        tool_inspection_manager.add_inspector(Box::new(
            crate::security::sensitive_ops::SensitiveOpsInspector,
        ));

        // Issue #63: route access to the machine-wide (global) memory store
        // through the user. Argument-aware — it reads `is_global`/`category`,
        // not just the tool name — and, unlike the sensitive-ops gate, active in
        // every mode that runs tools: Auto's blanket allow is the loudest case,
        // but a SmartApprove read-only grade and a user `AlwaysAllow` on the
        // memory tool are the same undisclosed cross-session read. Registered
        // above the permission inspector for readability only; the merge is
        // escalation-only, so its verdict wins from either position.
        tool_inspection_manager.add_inspector(Box::new(
            crate::security::global_memory::GlobalMemoryInspector,
        ));

        // Issue #56: refuse tool reads of the session database — `sessions.db`
        // and its `-wal`/`-shm` siblings, the transcript store every
        // conversation on this machine writes to. Every other privacy gate is a
        // gate on a Biorouter API; `cat`/`sqlite3`/`text_editor` on that file
        // walk around all of them. Path-argument aware, so a doc or a commit
        // message that merely names the path is untouched, and deliberately
        // blind to `privacy_tier`: reading the file discloses every other
        // session too, so the refusal is about the channel and fires for a
        // private-capability caller as well.
        tool_inspection_manager.add_inspector(Box::new(
            crate::security::session_store::SessionStoreInspector,
        ));

        // BR-71 §5: cross-session capability changes always confirm, in every
        // mode. Inert for every tool but `workspace_set_tools` and
        // `workspace_open`.
        tool_inspection_manager.add_inspector(Box::new(
            crate::agents::workspace_inspector::WorkspaceMutationInspector,
        ));

        // Add permission inspector (medium-high priority). BR-18: it reads the
        // shared risk registry the agent refreshes each turn from the model's
        // tool list, so `SmartApprove` auto-approves read-only-annotated tools
        // and only prompts on grades at/above the configured threshold.
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            tool_risks,
            permission_manager,
            managed,
            provider,
        )));

        // Add repetition inspector (lower priority - basic repetition checking).
        // BR-29: staged — a soft, non-blocking warning first, a hard stop only if
        // the model keeps repeating itself through it.
        // BR-30: plus the semantic heuristics (near-duplicate arg tweaks, A/B/A/B
        // oscillation), which are warn-only unless a hard stop is configured.
        let config = Config::global();
        let soft_warn_at = config
            .get_param::<u32>("BIOROUTER_REPETITION_SOFT_WARN")
            .unwrap_or(DEFAULT_REPETITION_SOFT_WARN);
        let hard_stop_at = config
            .get_param::<u32>("BIOROUTER_REPETITION_HARD_STOP")
            .unwrap_or(DEFAULT_REPETITION_HARD_STOP);
        tool_inspection_manager.add_inspector(Box::new(
            RepetitionInspector::staged(soft_warn_at, hard_stop_at)
                .with_semantic(Self::semantic_loop_config(config))
                .with_failure_loop(Self::failure_loop_config(config)),
        ));

        // Add user-configured PreToolUse hooks (runs last)
        tool_inspection_manager
            .add_inspector(Box::new(crate::hooks::HookInspector::new(hooks_manager)));

        tool_inspection_manager
    }

    /// BR-30: resolve the semantic loop-detection config.
    ///
    /// Defaults are deliberately warn-only — a heuristic that denies a call it
    /// misread is worse than one that nudges. Operators who want enforcement set
    /// `BIOROUTER_LOOP_NEAR_DUP_HARD_STOP` / `BIOROUTER_LOOP_OSCILLATION_HARD_STOP`
    /// (a value of 0 keeps the stage off).
    fn semantic_loop_config(config: &Config) -> SemanticLoopConfig {
        let defaults = SemanticLoopConfig::default();
        let positive = |value: u32| (value > 0).then_some(value);
        SemanticLoopConfig {
            enabled: config
                .get_param::<bool>("BIOROUTER_LOOP_SEMANTIC_DETECTION")
                .unwrap_or(defaults.enabled),
            similarity_threshold: config
                .get_param::<f32>("BIOROUTER_LOOP_ARG_SIMILARITY")
                .ok()
                .filter(|threshold| (0.0..=1.0).contains(threshold))
                .unwrap_or(defaults.similarity_threshold),
            near_dup_soft_warn: config
                .get_param::<u32>("BIOROUTER_LOOP_NEAR_DUP_SOFT_WARN")
                .ok()
                .map_or(defaults.near_dup_soft_warn, positive),
            near_dup_hard_stop: config
                .get_param::<u32>("BIOROUTER_LOOP_NEAR_DUP_HARD_STOP")
                .ok()
                .map_or(defaults.near_dup_hard_stop, positive),
            oscillation_soft_warn: config
                .get_param::<u32>("BIOROUTER_LOOP_OSCILLATION_SOFT_WARN")
                .ok()
                .map_or(defaults.oscillation_soft_warn, positive),
            oscillation_hard_stop: config
                .get_param::<u32>("BIOROUTER_LOOP_OSCILLATION_HARD_STOP")
                .ok()
                .map_or(defaults.oscillation_hard_stop, positive),
        }
    }

    /// BR-31: resolve the repeated-failing-result ("no progress") config.
    ///
    /// Unlike BR-30's heuristics this ships with its hard stage on: a run of
    /// identical *failures* is observed evidence, not a similarity guess. Each
    /// stage is individually disabled by setting it to 0
    /// (`BIOROUTER_FAILURE_LOOP_HARD_STOP=0` keeps the nudges but never blocks);
    /// `BIOROUTER_FAILURE_LOOP_DETECTION=false` turns the whole detector off.
    fn failure_loop_config(config: &Config) -> FailureLoopConfig {
        let defaults = FailureLoopConfig::default();
        let positive = |value: u32| (value > 0).then_some(value);
        FailureLoopConfig {
            enabled: config
                .get_param::<bool>("BIOROUTER_FAILURE_LOOP_DETECTION")
                .unwrap_or(defaults.enabled),
            similarity_threshold: config
                .get_param::<f32>("BIOROUTER_FAILURE_ERROR_SIMILARITY")
                .ok()
                .filter(|threshold| (0.0..=1.0).contains(threshold))
                .unwrap_or(defaults.similarity_threshold),
            soft_warn_at: config
                .get_param::<u32>("BIOROUTER_FAILURE_LOOP_SOFT_WARN")
                .ok()
                .map_or(defaults.soft_warn_at, positive),
            escalate_at: config
                .get_param::<u32>("BIOROUTER_FAILURE_LOOP_ESCALATE")
                .ok()
                .map_or(defaults.escalate_at, positive),
            hard_stop_at: config
                .get_param::<u32>("BIOROUTER_FAILURE_LOOP_HARD_STOP")
                .ok()
                .map_or(defaults.hard_stop_at, positive),
            // BR-51: opt in to hard-stopping a streak of *retryable* failures
            // (timeouts, transient dependency errors), which is off by default —
            // blocking the retry that would have worked is worse than one more.
            deny_retryable: config
                .get_param::<bool>("BIOROUTER_FAILURE_LOOP_DENY_RETRYABLE")
                .unwrap_or(defaults.deny_retryable),
        }
    }

    /// BR-31 result-collection seam: the escalating no-progress nudges owed to
    /// this batch's tool results.
    ///
    /// Called once the batch's results have been written into the response slots
    /// by [`Self::integrate_tool_result`], so the model sees "you have failed the
    /// same way 3 times" attached to the *third* failure — not one provider
    /// round-trip (and one more wasted call) later.
    ///
    /// `history` is the conversation as of the previous iteration; the outcomes of
    /// the batch that just ran are appended from the response slots, matching the
    /// exact same request→response pairing the inspector does on the transcript,
    /// so a streak spans iterations.
    async fn failure_loop_nudges(
        &self,
        history: &[Message],
        requests: &[ToolRequest],
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
    ) -> Vec<String> {
        if !self.failure_loop.enabled {
            return Vec::new();
        }

        let mut outcomes = crate::tool_monitor::tool_outcomes_since_last_user_turn(history);
        let mut failed_tools: Vec<String> = Vec::new();

        for request in requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            let Some(slot) = request_to_response_map.get(&request.id) else {
                continue;
            };
            let response = slot.lock().await.clone();
            let Some(outcome) = crate::tool_monitor::outcome_from_response_message(
                &tool_call.name,
                &request.id,
                &response,
            ) else {
                continue;
            };
            if outcome.failure.is_some() && !failed_tools.contains(&outcome.tool_name) {
                failed_tools.push(outcome.tool_name.clone());
            }
            outcomes.push(outcome);
        }

        failed_tools
            .iter()
            .filter_map(|tool_name| {
                let nudge = crate::tool_monitor::failure_loop_nudge(
                    &self.failure_loop,
                    &outcomes,
                    tool_name,
                )?;
                // BR-67: the nudge is a loop-safety decision; put which tool has
                // been failing, and how long its streak is, on the record.
                loop_safety::emit(
                    LoopSafetyEvent::new(LoopSafetyKind::FailureLoopNudge)
                        .tool(tool_name)
                        .count(crate::tool_monitor::failing_streak(
                            &outcomes,
                            tool_name,
                            self.failure_loop.similarity_threshold,
                        )),
                );
                Some(nudge)
            })
            .collect()
    }

    /// BR-66: this batch's outcomes as the general mistake-streak counter sees
    /// them — one entry per tool call the *model* is answerable for, in request
    /// order.
    ///
    /// Two deliberate differences from BR-31's view of the same batch:
    ///
    /// * A **malformed** tool call (one the provider emitted that never parsed)
    ///   counts as a mistake. BR-31 skips it — it has no tool name to key a
    ///   per-tool failure streak on — but "the model keeps emitting garbage
    ///   calls" is exactly the streak BR-66 exists to catch.
    /// * Calls that never ran because a **guard denied** them, or because the
    ///   **user declined** them, are dropped. Those are policy verdicts, not the
    ///   model's failures; counting them would nudge the model for a decision it
    ///   did not make, on top of the warning BR-29/30/31 already sent.
    async fn mistake_outcomes(
        &self,
        requests: &[ToolRequest],
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
    ) -> Vec<crate::tool_monitor::ToolOutcome> {
        let denied: HashSet<&str> = permission_check_result
            .denied
            .iter()
            .map(|request| request.id.as_str())
            .collect();

        let mut outcomes = Vec::new();
        for request in requests {
            if denied.contains(request.id.as_str()) {
                continue;
            }
            match &request.tool_call {
                Ok(tool_call) => {
                    let Some(slot) = request_to_response_map.get(&request.id) else {
                        continue;
                    };
                    let response = slot.lock().await.clone();
                    let Some(outcome) = crate::tool_monitor::outcome_from_response_message(
                        &tool_call.name,
                        &request.id,
                        &response,
                    ) else {
                        continue;
                    };
                    if crate::agents::mistakes::is_user_decline(&outcome) {
                        continue;
                    }
                    outcomes.push(outcome);
                }
                Err(error) => outcomes.push(crate::tool_monitor::ToolOutcome {
                    tool_name: crate::agents::mistakes::MALFORMED_TOOL_NAME.to_string(),
                    failure: Some(error.message.to_string()),
                    // BR-51: a call the model emitted malformed never reached a
                    // tool — the arguments themselves were the failure.
                    kind: Some(crate::agents::tool_errors::ToolErrorKind::InvalidArgs),
                }),
            }
        }
        outcomes
    }

    /// Reset the retry attempts counter to 0
    pub async fn reset_retry_attempts(&self) {
        self.retry_manager.reset_attempts().await;
    }

    /// Increment the retry attempts counter and return the new value
    pub async fn increment_retry_attempts(&self) -> u32 {
        self.retry_manager.increment_attempts().await
    }

    /// Get the current retry attempts count
    pub async fn get_retry_attempts(&self) -> u32 {
        self.retry_manager.get_attempts().await
    }

    async fn handle_retry_logic(
        &self,
        messages: &mut Conversation,
        session_config: &SessionConfig,
        initial_messages: &[Message],
    ) -> Result<bool> {
        let result = self
            .retry_manager
            .handle_retry_logic(
                messages,
                session_config,
                initial_messages,
                &self.final_output_tool,
            )
            .await?;

        match result {
            RetryResult::Retried => Ok(true),
            RetryResult::Skipped
            | RetryResult::MaxAttemptsReached
            | RetryResult::SuccessChecksPassed => Ok(false),
        }
    }
    async fn drain_elicitation_messages(&self, session_id: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        let manager = self.config.session_manager.clone();
        // Only requests deliverable to THIS session (its own scope plus the
        // unscoped fallback) — a request scoped to a concurrent session stays
        // queued for that session's loop, so its prompt is never persisted or
        // yielded under the wrong session id (#40).
        for mut elicitation_message in ActionRequiredManager::global().drain_requests(session_id) {
            // #41: adopt the minted uid so the yielded copy matches the row.
            if let Err(e) = manager
                .add_message_adopting_uid(session_id, &mut elicitation_message)
                .await
            {
                warn!("Failed to save elicitation message to session: {}", e);
            }
            messages.push(elicitation_message);
        }
        messages
    }

    async fn prepare_reply_context(
        &self,
        session_id: &str,
        unfixed_conversation: Conversation,
        working_dir: &std::path::Path,
    ) -> Result<ReplyContext> {
        // BR-56: the pre-fix copy exists only to render the debug diff, so only pay
        // for it when that log line will actually be emitted. The `Conversation`
        // clone itself is now a refcount bump.
        let unfixed_messages =
            tracing::enabled!(tracing::Level::DEBUG).then(|| unfixed_conversation.clone());
        let (conversation, issues) = self.normalizer.normalize(unfixed_conversation);
        if !issues.is_empty() {
            if let Some(unfixed) = &unfixed_messages {
                debug!(
                    "Conversation issue fixed: {}",
                    debug_conversation_fix(unfixed.messages(), conversation.messages(), &issues)
                );
            }
        }
        // Cheap now that the transcript is Arc-shared: this is the pre-turn
        // snapshot the retry path restores from.
        let initial_messages = conversation.clone();

        let mut conversation = conversation;
        if let Some(context) = self
            .explicit_resource_context(session_id, conversation.messages())
            .await
        {
            conversation.push(
                Message::user()
                    .with_text(format!(
                        "<explicit-resource-context>\n{context}\n</explicit-resource-context>"
                    ))
                    .with_visibility(false, true),
            );
        }

        let (tools, toolshim_tools, system_prompt) = self
            .prepare_tools_and_prompt(session_id, working_dir)
            .await?;

        Ok(ReplyContext {
            conversation,
            tools,
            toolshim_tools,
            system_prompt,
            biorouter_mode: self.config.biorouter_mode,
            initial_messages,
        })
    }

    async fn explicit_resource_context(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Option<String> {
        let latest_user_text = messages
            .iter()
            .rev()
            .find(|message| {
                message.role == rmcp::model::Role::User && message.metadata.user_visible
            })
            .map(Message::as_concat_text)?;

        let refs = extract_resource_refs(&latest_user_text);
        if refs.is_empty() {
            return None;
        }

        let mut sections = Vec::new();

        if !refs.skills.is_empty() {
            sections.push(self.skill_resource_context(session_id, &refs).await);
        }

        if !refs.extensions.is_empty() {
            sections.push(
                self.extension_resource_context(session_id, &refs.extensions)
                    .await,
            );
        }

        if !refs.knowledge_bases.is_empty() {
            sections.push(
                self.knowledge_resource_context(session_id, &latest_user_text, &refs)
                    .await,
            );
        }

        Some(sections.join("\n\n"))
    }

    async fn skill_resource_context(&self, session_id: &str, refs: &ResourceRefs) -> String {
        let mut output = format!(
            "The user explicitly selected these skills for this request: {}.\n\
             Treat these selected skills as mandatory. Use the loaded skill instructions below before answering or taking action.",
            refs.skills
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        for skill in &refs.skills {
            output.push_str(&format!("\n\n## Loaded skill: {skill}\n"));

            // BR-8: a skill body is inlined in full only the first turn it is
            // selected. On later turns the full text (megabytes across a
            // skill-heavy session) is replaced by a short pointer — the skill
            // stays mandatory and the model can re-read it on demand.
            if self.skill_already_injected(session_id, skill).await {
                output.push_str(skill_already_loaded_pointer());
                continue;
            }

            match self
                .call_prefetch_tool(
                    session_id,
                    "skills__loadSkill",
                    object!({ "name": skill.clone() }),
                )
                .await
            {
                Ok(text) => {
                    // Cap a single body against the BR-2 injection budget so a
                    // pathological SKILL.md can't blow the window on its own.
                    output.push_str(&crate::context_budget::truncate_to_tokens(
                        &text,
                        crate::context_budget::max_skill_body_tokens(),
                        &format!("selected skill `{skill}`"),
                    ));
                    self.mark_skill_injected(session_id, skill).await;
                }
                Err(error) => output.push_str(&format!(
                    "Could not load this selected skill: {error}. Tell the user instead of silently substituting another skill."
                )),
            }
        }

        output
    }

    /// Whether `skill`'s full body was already inlined earlier in this session
    /// (BR-8 cache).
    async fn skill_already_injected(&self, session_id: &str, skill: &str) -> bool {
        self.injected_skills
            .lock()
            .await
            .get(session_id)
            .is_some_and(|injected| injected.contains(skill))
    }

    /// Record that `skill`'s full body has now been inlined for this session, so
    /// later turns inject only a pointer (BR-8).
    async fn mark_skill_injected(&self, session_id: &str, skill: &str) {
        self.injected_skills
            .lock()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(skill.to_string());
    }

    async fn extension_resource_context(&self, session_id: &str, extensions: &[String]) -> String {
        let mut selected = Vec::new();
        let mut notes = Vec::new();

        for requested in extensions {
            let requested = requested.trim();
            if requested.is_empty() {
                continue;
            }

            // Resolve by extension id and owning registry, never by display
            // name: two platform extensions are registered under a display
            // string ("Extension Manager", "Chat Recall"), and feeding that into
            // the *builtin* lookup made a valid `/ext:` request fail exactly
            // like a policy refusal (issue #48).
            let target = resolve_bundled_extension(requested);
            // The key the extension is stored under is also its tool prefix, so
            // it is what the model is told to use below.
            let canonical = target
                .as_ref()
                .map(|target| target.key())
                .unwrap_or_else(|| normalize(requested));

            let target_is_enabled = if let Some(target) = target.as_ref() {
                self.extension_manager
                    .is_bundled_target_enabled(target)
                    .await
            } else {
                self.extension_manager
                    .is_extension_enabled(&canonical)
                    .await
            };

            if !target_is_enabled {
                if let Some(target) = target {
                    if self
                        .extension_manager
                        .is_extension_enabled(&canonical)
                        .await
                    {
                        notes.push(format!(
                            "`{canonical}` is occupied by a different extension, so the selected bundled extension was not enabled."
                        ));
                        continue;
                    }
                    let config = target.clone().into_config(format!(
                        "Selected via explicit resource marker /ext:{canonical}"
                    ));
                    match self.add_extension(config).await {
                        Ok(()) => {
                            if let Err(error) = self.persist_extension_state(session_id).await {
                                notes.push(format!(
                                    "`{canonical}` was enabled for this turn but its session state could not be persisted: {error}"
                                ));
                            } else {
                                notes.push(format!(
                                    "`{canonical}` was enabled because the user selected it explicitly."
                                ));
                            }
                        }
                        Err(error) => notes.push(format!(
                            "`{canonical}` could not be enabled: {error}. Tell the user instead of silently substituting another extension."
                        )),
                    }
                    if !self
                        .extension_manager
                        .is_bundled_target_enabled(&target)
                        .await
                    {
                        continue;
                    }
                } else {
                    notes.push(format!(
                        "`{canonical}` is not currently enabled and is not a known built-in extension. Tell the user instead of silently substituting another extension."
                    ));
                    continue;
                }
            }

            selected.push(canonical);
        }

        let mut output = format!(
            "The user explicitly selected these extensions for this request: {}.\n\
             Treat these selected extensions as mandatory. Use tools from these extensions when the request needs tool use. Tool names are prefixed with the extension name and `__`; if a selected extension is unavailable, say so plainly.",
            selected
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        if !notes.is_empty() {
            output.push_str("\n\n");
            output.push_str(&notes.join("\n"));
        }

        output
    }

    async fn knowledge_resource_context(
        &self,
        session_id: &str,
        user_text: &str,
        refs: &ResourceRefs,
    ) -> String {
        let mut output = String::from(
            "The user explicitly selected the following knowledge base(s). Use these results as the primary knowledge context for this request. If more context is needed, call `knowledge__kb_search` with the same exact `kb_id`.",
        );

        for kb in &refs.knowledge_bases {
            output.push_str(&format!("\n\n## Knowledge base: `{}`\n", kb.id));
            match self
                .call_prefetch_tool(
                    session_id,
                    "knowledge__kb_search",
                    object!({
                        "kb_id": kb.id.clone(),
                        "query": user_text,
                        "limit": 5
                    }),
                )
                .await
            {
                Ok(text) => output.push_str(&text),
                Err(error) => output.push_str(&format!(
                    "Could not search this selected knowledge base: {error}. Tell the user instead of silently searching a different knowledge base."
                )),
            }
        }

        output
    }

    async fn call_prefetch_tool(
        &self,
        session_id: &str,
        tool_name: &str,
        arguments: serde_json::Map<String, Value>,
    ) -> Result<String> {
        // Issue #56: one of the four production entries that sample a capability.
        // The pre-turn prefetch is its own entry because it dispatches outside
        // `Self::dispatch_tool_call` entirely.
        let cap = crate::privacy::CallCapability::sample(&self.provider).await;
        let tool = self
            .extension_manager
            .dispatch_tool_call(
                session_id,
                CallToolRequestParams {
                    name: tool_name.to_string().into(),
                    arguments: Some(arguments),
                    meta: None,
                    task: None,
                },
                cap,
                CancellationToken::default(),
            )
            .await
            .map_err(|e| anyhow!(e.to_string()))?;

        let result = tool.result.await.map_err(|e| anyhow!(e.message))?;
        let text = result
            .content
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n\n");

        if text.is_empty() {
            Ok("(The selected resource returned no text.)".to_string())
        } else {
            Ok(text)
        }
    }

    async fn categorize_tools(
        &self,
        response: &Message,
        tools: &[rmcp::model::Tool],
    ) -> ToolCategorizeResult {
        // Categorize tool requests
        let (frontend_requests, remaining_requests, filtered_response) =
            self.categorize_tool_requests(response, tools).await;

        ToolCategorizeResult {
            frontend_requests,
            remaining_requests,
            filtered_response,
        }
    }

    /// Assemble the per-turn model context by injecting MOIM ("message of the
    /// moment") into a clone of the live conversation, leaving persisted history untouched.
    ///
    /// BR-56: the transcript is normalized here — i.e. before *every* provider
    /// call, not once per reply. Inside a multi-tool turn the loop appends
    /// assistant/tool messages between provider calls, so a reply-time-only fix
    /// let the next call receive an un-normalized suffix (e.g. two consecutive
    /// assistant messages). MOIM injection already re-normalized on the way
    /// through; this closes the same hole for sessions with no MOIM provider.
    /// `BIOROUTER_NORMALIZE_EACH_TURN=false` restores the old behaviour.
    async fn assemble_turn_context(
        &self,
        session_id: &str,
        conversation: &Conversation,
        working_dir: &std::path::Path,
    ) -> Conversation {
        let _phase = super::phase_timing::Phase::start("agent.assemble_turn_context");

        let moim_phase = super::phase_timing::Phase::start("agent.inject_moim");
        let (conversation, moim_injected) = super::moim::inject_moim(
            session_id,
            conversation.clone(),
            &self.extension_manager,
            working_dir,
            &self.normalizer,
        )
        .await;
        drop(moim_phase);

        if moim_injected || !normalize_each_turn() {
            // MOIM injection normalizes on its way through.
            return conversation;
        }
        let _normalize_phase = super::phase_timing::Phase::start("agent.normalize_conversation");
        self.normalizer.normalize(conversation).0
    }

    /// Run the per-tool inspection gauntlet (inspectors → permission judge →
    /// extension-enable tracking) and eagerly dispatch approved/denied tools,
    /// returning the inspection results, permission verdict, enable-extension
    /// request ids, and the pending tool futures.
    ///
    /// BR-19: `remaining_requests` is `&mut` because a PreToolUse hook may
    /// *rewrite* a tool's input (sandbox a path, redact a payload, normalize a
    /// command). The rewrite is applied here — after the hooks ran, before
    /// anything is dispatched — and the requests the caller goes on to persist
    /// are the rewritten ones, so the transcript matches what actually executed.
    async fn inspect_and_gate_tool_requests(
        &self,
        remaining_requests: &mut Vec<ToolRequest>,
        conversation: &Conversation,
        biorouter_mode: BioRouterMode,
        session: &Session,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<(
        Vec<InspectionResult>,
        PermissionCheckResult,
        Vec<String>,
        Vec<(String, ToolStream)>,
    )> {
        // Sync the developer server's `text_editor` path jail to the live mode
        // before anything is dispatched. In Auto ("Fully Automatic") mode the
        // jail is relaxed so legitimate writes outside the session working dir
        // (e.g. `/tmp`) are not rejected — sensitive-path writes stay gated by
        // the `SensitiveOpsInspector` above. Every other mode keeps the jail.
        // The policy (Auto ⇒ relaxed) lives here; the developer server only
        // reads the flag. See `biorouter_mcp::set_path_jail_relaxed`.
        biorouter_mcp::set_path_jail_relaxed(biorouter_mode == BioRouterMode::Auto);

        // Run all tool inspectors
        let mut inspection_results = self
            .tool_inspection_manager
            .inspect_tools(
                remaining_requests,
                conversation.messages(),
                biorouter_mode,
                session,
            )
            .await?;

        // BR-19: apply any tool-input rewrite the PreToolUse hooks staged. The
        // rewritten input has NOT been seen by the security/permission
        // inspectors (they ran above, on the model's original arguments), so a
        // rewrite must not be a hole around them: re-run every inspector except
        // the hook one (re-running that would execute the user's hook commands
        // twice and let a rewrite trigger another rewrite).
        let rewrites = self.hooks_manager.take_tool_input_rewrites(&session.id);
        if !rewrites.is_empty()
            && crate::hooks::apply_tool_input_rewrites(remaining_requests, &rewrites) > 0
        {
            let mut revalidated = self
                .tool_inspection_manager
                .inspect_tools_excluding(
                    &[crate::hooks::inspector::HOOK_INSPECTOR_NAME],
                    remaining_requests,
                    conversation.messages(),
                    biorouter_mode,
                    session,
                )
                .await?;
            inspection_results.retain(|result| {
                result.inspector_name == crate::hooks::inspector::HOOK_INSPECTOR_NAME
            });
            inspection_results.append(&mut revalidated);
        }

        let permission_check_result = self
            .tool_inspection_manager
            .process_inspection_results_with_permission_inspector(
                remaining_requests,
                &inspection_results,
            )
            .unwrap_or_else(|| {
                let mut result = PermissionCheckResult {
                    approved: vec![],
                    needs_approval: vec![],
                    denied: vec![],
                };
                result
                    .needs_approval
                    .extend(remaining_requests.iter().cloned());
                result
            });

        // Track extension requests
        let mut enable_extension_request_ids = vec![];
        for request in remaining_requests {
            if let Ok(tool_call) = &request.tool_call {
                if tool_call.name == MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE {
                    enable_extension_request_ids.push(request.id.clone());
                }
            }
        }

        let tool_futures = self
            .handle_approved_and_denied_tools(
                &permission_check_result,
                request_to_response_map,
                cancel_token,
                session,
                &inspection_results,
            )
            .await?;

        Ok((
            inspection_results,
            permission_check_result,
            enable_extension_request_ids,
            tool_futures,
        ))
    }

    /// Integrate one completed tool result: validate it before persistence,
    /// classify a failure (BR-51), note extension-install failures, record it for
    /// PostToolUse hooks, and write it into the request's response slot.
    #[allow(clippy::too_many_arguments)]
    async fn integrate_tool_result(
        &self,
        request_id: String,
        output: ToolResult<CallToolResult>,
        enable_extension_request_ids: &[String],
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        request_to_tool_name: &HashMap<String, String>,
        request_to_original_tool_call: &HashMap<String, CallToolRequestParams>,
        request_to_executed_tool_call: &HashMap<String, CallToolRequestParams>,
        request_metadata: &HashMap<String, Option<ProviderMetadata>>,
        all_install_successful: &mut bool,
        post_tool_results: &mut Vec<(String, Option<Value>, Option<String>)>,
        tool_output_guardrail: crate::guardrails::tool_output::ToolOutputGuardrailMode,
        tool_error_taxonomy: crate::agents::tool_errors::ToolErrorTaxonomyConfig,
    ) {
        let _phase = super::phase_timing::Phase::start("agent.integrate_tool_result");
        let output = call_tool_result::validate(output);
        let output_was_err = output.is_err();
        let execution_audit = if let (Some(original), Some(executed)) = (
            request_to_original_tool_call.get(&request_id),
            request_to_executed_tool_call.get(&request_id),
        ) {
            let original = serde_json::to_value(original).ok();
            let executed = serde_json::to_value(executed).ok();
            if original != executed {
                Some(serde_json::json!({
                    "providerAuthored": original,
                    "actuallyExecuted": executed,
                }))
            } else {
                None
            }
        } else {
            None
        };

        // Frame tool output as untrusted data before it re-enters the model
        // context, and scan it for injection markers + PII/PHI. The frame is
        // unconditional (provenance, not phrase matching; see the module
        // docs); the scan only escalates. Never blocks or drops content;
        // masking is opt-in. Off is a zero-cost pass-through.
        let (output, guardrail_summary) = crate::guardrails::tool_output::guard_tool_result(
            output,
            request_to_tool_name.get(&request_id).map(String::as_str),
            tool_output_guardrail,
        );
        if let Some(summary) = &guardrail_summary {
            debug!(request_id = %request_id, "tool-output guardrail flagged: {summary}");
        }

        // BR-51: a failure is classified once, here — the single funnel every
        // completed tool result passes through. The envelope rides on the result
        // (so the GUI and a reloaded session get it) and the typed header rides
        // in the text (so the model, and the BR-31/66 detectors reading the
        // transcript back, can tell a retryable blip from a hard failure).
        let (mut output, tool_error) =
            crate::agents::tool_errors::annotate_tool_result(output, tool_error_taxonomy);
        if let Some(audit) = execution_audit {
            match &mut output {
                Ok(result) => {
                    let meta = result.meta.get_or_insert_with(rmcp::model::Meta::new);
                    meta.0.insert("biorouterToolExecution".to_string(), audit);
                }
                Err(error) => {
                    let mut data = match error.data.take() {
                        Some(Value::Object(data)) => data,
                        Some(data) => {
                            serde_json::Map::from_iter([("providerErrorData".to_string(), data)])
                        }
                        None => serde_json::Map::new(),
                    };
                    data.insert("biorouterToolExecution".to_string(), audit);
                    error.data = Some(Value::Object(data));
                }
            }
        }
        if let Some(error) = &tool_error {
            debug!(
                request_id = %request_id,
                kind = error.kind.as_str(),
                retryable = error.retryable,
                "tool call failed"
            );
        }

        if enable_extension_request_ids.contains(&request_id) && output_was_err {
            *all_install_successful = false;
        }
        {
            let (response_value, error_text) = match &output {
                Ok(res) => {
                    let value = serde_json::to_value(res).ok();
                    if res.is_error == Some(true) {
                        let text = value
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "tool returned an error".to_string());
                        (value, Some(text))
                    } else {
                        (value, None)
                    }
                }
                Err(e) => (None, Some(e.to_string())),
            };
            post_tool_results.push((request_id.clone(), response_value, error_text));
        }
        if let Some(response_msg) = request_to_response_map.get(&request_id) {
            let metadata = request_metadata.get(&request_id).and_then(|m| m.as_ref());
            let mut response = response_msg.lock().await;
            *response = response
                .clone()
                .with_tool_response_with_metadata(request_id, output, metadata);
        }
    }

    /// BR-32 loop seam: the periodic "are you looping?" progress check, and the
    /// staged response to its verdict.
    ///
    /// The `/goal` loop has had real stall detection for a while (fuzzy
    /// similarity of the judge's feedback across attempts, a counter that does
    /// NOT reset when tools run, a graceful give-up) — but only for sessions with
    /// a goal set, which is not where most stuck loops happen. This runs the same
    /// idea for *every* session, on a schedule: nothing until a single turn has
    /// already burned [`stall::StallCheckConfig::first_check_at`] actions without
    /// returning to the user, then one small fast-model call every
    /// `interval` actions. See [`crate::agents::stall`].
    ///
    /// Skipped when a `/goal` is active: that session already pays for an LLM
    /// judge on every stop attempt and owns a non-resetting stall budget, so a
    /// second detector would double the cost and could fight the goal's own
    /// give-up. Fail-open everywhere — no tail, no provider, a provider error, or
    /// an unreadable verdict all mean [`StallAction::Proceed`].
    async fn stall_check(
        &self,
        session_id: &str,
        conversation: &Conversation,
        actions_taken: u32,
        config: &StallCheckConfig,
        watch: &mut StallWatch,
    ) -> StallAction {
        if !config.due(actions_taken) || watch.has_given_up() {
            return StallAction::Proceed;
        }
        if self.active_goal(session_id).await.is_some() {
            return StallAction::Proceed;
        }
        let Some(tail) = crate::agents::stall::progress_tail(conversation) else {
            return StallAction::Proceed;
        };
        let Ok(provider) = self.provider().await else {
            return StallAction::Proceed;
        };
        let verdict = crate::agents::stall::check_progress(
            provider,
            &tail,
            actions_taken,
            watch.last_reason(),
        )
        .await;
        watch.record(verdict.as_deref(), config)
    }

    /// BR-19: honor a PostToolUse / PostToolUseFailure `block` decision.
    ///
    /// PostToolUse hooks were observe-only: the decision was computed and thrown
    /// away, so a hook could not reject e.g. a write that fails lint. It is now
    /// applied to the already-integrated tool response, in place — the tool has
    /// *run*, so its output is kept (its side effects stand and the model may
    /// need to see what happened) and the hook's reason is appended as corrective
    /// feedback, with the result marked as an error so the model treats it as a
    /// failure to address rather than a success to build on.
    async fn apply_post_tool_block(
        &self,
        request_id: &str,
        tool_name: &str,
        reason: &str,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
    ) {
        let Some(response_msg) = request_to_response_map.get(request_id) else {
            return;
        };
        let mut response = response_msg.lock().await;
        for content in response.content.iter_mut() {
            let MessageContent::ToolResponse(tool_response) = content else {
                continue;
            };
            if tool_response.id != request_id {
                continue;
            }
            let feedback = format!(
                "A PostToolUse hook blocked this result for `{tool_name}`.\n\n\
                 Hook feedback: {reason}\n\n\
                 The tool already ran, so its side effects stand. Address the feedback \
                 before continuing; do not simply retry the identical call."
            );
            match &mut tool_response.tool_result {
                Ok(result) => {
                    result.content.push(Content::text(feedback));
                    result.is_error = Some(true);
                }
                Err(error) => {
                    error.message = format!("{}\n\n{feedback}", error.message).into();
                }
            }
        }
    }

    /// Record this turn's provider usage exactly once for token accounting
    /// (no-op when the turn reported none, e.g. an error before the first usage chunk).
    ///
    /// BR-35: the same usage also feeds the per-reply budget, which is the only
    /// thing that sees the *whole reply's* spend — the session gauge tracks the
    /// live context, not what this reply has burned. Pricing is looked up per
    /// turn against the model that actually ran (a lead/worker swap mid-reply is
    /// therefore priced correctly), and only when a dollar limit is set.
    /// Returns `true` when it actually wrote the session's counters, so the
    /// caller knows a fresh [`AgentEvent::TokenUsage`] is worth emitting (BR-52).
    async fn record_turn_usage(
        &self,
        session_config: &SessionConfig,
        turn_usage: Option<crate::providers::base::ProviderUsage>,
        budget: &mut BudgetTracker,
        event_key: &str,
    ) -> Result<bool> {
        if let Some(usage) = turn_usage {
            self.record_budget_usage(budget, &usage).await;
            self.update_session_metrics(session_config, &usage, false, event_key)
                .await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// BR-52: the session's token counters as the agent last wrote them.
    ///
    /// Read exactly once per turn/compaction boundary (where the counters can
    /// actually change) and carried in the event stream, instead of the server
    /// re-reading SQLite for every streamed chunk. Best-effort: a failed read
    /// yields `None` and the consumer simply keeps the state it already has.
    pub(super) async fn current_token_state(&self, session_id: &str) -> Option<TokenState> {
        match self
            .config
            .session_manager
            .get_token_counts(session_id)
            .await
        {
            Ok(counts) => Some(TokenState::from(counts)),
            Err(e) => {
                warn!("Failed to read token counts for session {session_id}: {e}");
                None
            }
        }
    }

    /// Fold one provider round-trip (a turn, or an in-reply compaction) into the
    /// BR-35 budget. Free when no budget is set.
    async fn record_budget_usage(
        &self,
        budget: &mut BudgetTracker,
        usage: &crate::providers::base::ProviderUsage,
    ) {
        if !budget.is_active() {
            return;
        }
        let provider_name = match self.provider().await {
            Ok(provider) => provider.get_name().to_string(),
            // No provider is a pricing miss, not a stop: the token and clock
            // axes still hold.
            Err(_) => String::new(),
        };
        budget.record_usage(&provider_name, usage);
    }

    /// BR-13 overflow recovery, persisted safely against a concurrent writer.
    ///
    /// Compacts `conversation` with `recovery`, then swaps it in under the
    /// store's freshness guard. If the basis moved out from under us the
    /// compaction is recomputed once against the fresh history and retried;
    /// bounded at one retry because every attempt re-spends a billed
    /// summarization call.
    ///
    /// Never returns `Err` for a lost race. It also never returns `Err` once
    /// the FIRST summarization has been billed: past that point a failure —
    /// either summarizer, or the database on either attempt — degrades to
    /// `stored: None` with `usages` intact, so the spend is still reported and
    /// the turn still has a compaction to continue from. Only a failure before
    /// anything was charged propagates, because there is then nothing to
    /// salvage. This site is the last rung before the turn dies with "context
    /// limit still exceeded", so declining to persist must not also decline to
    /// continue.
    ///
    /// `basis` is `&mut` because a landed swap invalidates it — the rewrite
    /// renumbered every row — and re-seeding it is this function's job, not the
    /// caller's. Doing it here is what keeps the revision and the history it
    /// describes moving together (see [`RewriteBasis`]).
    async fn swap_overflow_compaction(
        &self,
        session_id: &str,
        conversation: &Conversation,
        basis: &mut RewriteBasis,
        recovery: crate::context_mgmt::OverflowRecovery,
    ) -> Result<OverflowCompactionSwap> {
        let session_manager = self.config.session_manager.clone();
        let mut usages = Vec::new();
        let raw_conversation = basis.raw_with_new_durable_messages(conversation);

        let (compacted, usage) = compact_messages_with_recovery(
            self.provider().await?.as_ref(),
            &raw_conversation,
            recovery,
        )
        .await?;
        usages.push(usage);

        let first_attempt = session_manager
            .replace_conversation_preserving_tail(
                session_id,
                &compacted,
                basis.revision,
                &raw_conversation,
            )
            .await;
        // The summarization above is BILLED — the provider charged for it the
        // moment it answered — so nothing from here on may leave through `?`.
        // The caller bills only the `Ok` branch and breaks on `Err`, so an
        // `await?` here loses the spend from both the budget and the session
        // gauge, leaves the `PreCompact` it fired without its `PostCompact`, and
        // ends a turn that still had a usable compaction in memory — at the last
        // rung before "context limit still exceeded". Degrade to "could not
        // persist", exactly as the retry path already does.
        //
        // It does NOT fall through to the retry: a store error is not a moved
        // basis, so there is nothing to recompute against and re-spending a
        // second summarization would buy nothing.
        let (outcome, stored) = match first_attempt {
            Ok(result) => result,
            Err(e) => {
                warn!(
                    "Could not persist the overflow-recovery compaction for session \
                     {session_id} ({e}); continuing in memory with the stored history intact"
                );
                return Ok(OverflowCompactionSwap::unpersisted(compacted, usages));
            }
        };
        if outcome.stored() {
            return Ok(OverflowCompactionSwap {
                stored: Some(self.reseed_basis(session_id, basis, stored).await),
                compacted,
                usages,
            });
        }

        warn!(
            "Overflow-recovery compaction for session {session_id} was declined ({outcome:?}); \
             recomputing against the current history and retrying once"
        );
        self.retry_overflow_compaction(session_id, basis, recovery, compacted, usages)
            .await
    }

    /// The second and last attempt behind [`Self::swap_overflow_compaction`],
    /// reached only when the first was DECLINED (the stored history moved, so
    /// there is a fresh basis to recompute against).
    ///
    /// Same rule as the first attempt — `compacted` is a perfectly usable
    /// in-memory result and `usages` is already-charged spend, so every failure
    /// here degrades to [`OverflowCompactionSwap::unpersisted`] rather than
    /// propagating.
    ///
    /// It re-seeds `basis` BEFORE recomputing: the decline means the stored
    /// history moved, so the pair the retry writes against has to be re-read as
    /// a PAIR, never a fresh revision against the stale view.
    async fn retry_overflow_compaction(
        &self,
        session_id: &str,
        basis: &mut RewriteBasis,
        recovery: crate::context_mgmt::OverflowRecovery,
        compacted: Conversation,
        mut usages: Vec<crate::providers::base::ProviderUsage>,
    ) -> Result<OverflowCompactionSwap> {
        let session_manager = self.config.session_manager.clone();

        match RewriteBasis::read(&session_manager, session_id).await {
            Ok(fresh) => *basis = fresh,
            Err(e) => {
                warn!(
                    "Could not re-read session {session_id} to retry the overflow-recovery \
                     compaction ({e}); continuing in memory with the first compaction"
                );
                return Ok(OverflowCompactionSwap::unpersisted(compacted, usages));
            }
        }
        let fresh = basis.known().clone();
        let provider = match self.provider().await {
            Ok(provider) => provider,
            Err(e) => {
                warn!(
                    "No provider to retry the overflow-recovery compaction for session \
                     {session_id} ({e}); continuing in memory with the first compaction"
                );
                return Ok(OverflowCompactionSwap::unpersisted(compacted, usages));
            }
        };
        let (recompacted, retry_usage) =
            match compact_messages_with_recovery(provider.as_ref(), &fresh, recovery).await {
                Ok(result) => result,
                Err(e) => {
                    warn!(
                        "Retry summarization for session {session_id} failed ({e}); continuing \
                         in memory with the first compaction"
                    );
                    return Ok(OverflowCompactionSwap::unpersisted(compacted, usages));
                }
            };
        usages.push(retry_usage);

        match session_manager
            .replace_conversation_preserving_tail(session_id, &recompacted, basis.revision, &fresh)
            .await
        {
            Ok((retry_outcome, retry_stored)) => Ok(OverflowCompactionSwap {
                stored: match retry_outcome.stored() {
                    true => Some(self.reseed_basis(session_id, basis, retry_stored).await),
                    false => None,
                },
                compacted: recompacted,
                usages,
            }),
            // The write failed rather than being declined. Same shape as the
            // declined case one line up — including keeping `recompacted`, the
            // compaction of the store's CURRENT history, over the staler
            // `compacted`.
            Err(e) => {
                warn!(
                    "Could not persist the retried overflow-recovery compaction for session \
                     {session_id} ({e}); continuing in memory with the stored history intact"
                );
                Ok(OverflowCompactionSwap::unpersisted(recompacted, usages))
            }
        }
    }

    /// Re-seed `basis` after a rewrite landed, and hand back the conversation
    /// the turn should continue from.
    ///
    /// A landed rewrite renumbered every row, so the old basis describes a store
    /// that no longer exists. The replacement has to be a PAIR again: re-reading
    /// only the revision — against the `stored` view the rewrite returned, which
    /// was fixed at commit time — leaves anything appended in between counted by
    /// the new watermark yet absent from the turn's view, i.e. exactly the row
    /// the next swap would delete without recovering. So the turn continues from
    /// the re-read history rather than from `stored`: it is `stored` plus
    /// whatever landed since, which the next compaction then summarizes instead
    /// of destroying.
    ///
    /// If the re-read fails, `basis` is deliberately left stale. A stale basis
    /// fails the store's prefix check, so the next swap this turn is DECLINED
    /// (and retried against a fresh pair) — the safe outcome. Guessing a
    /// revision for a view we no longer trust is the unsafe one.
    async fn reseed_basis(
        &self,
        session_id: &str,
        basis: &mut RewriteBasis,
        stored: Conversation,
    ) -> Conversation {
        match RewriteBasis::read(&self.config.session_manager, session_id).await {
            Ok(fresh) => {
                let adopted = fresh.known().clone();
                *basis = fresh;
                adopted
            }
            Err(e) => {
                warn!(
                    "Could not re-read session {session_id} after its history was replaced ({e}); \
                     a further compaction this turn will be declined rather than written against \
                     a basis that no longer describes the store"
                );
                stored
            }
        }
    }

    /// BR-28: the turn-boundary settle for observe-only (`fire`d) hook events —
    /// Notification, SubagentStart/Stop, Pre/PostCompact.
    ///
    /// Those hooks used to be spawned detached with their whole `HookAggregate`
    /// dropped, so a `systemMessage` was invisible, a failing hook untraceable,
    /// and the task could outlive the turn. Now the boundary joins whatever has
    /// finished (bounded by [`crate::hooks::FIRE_JOIN_BUDGET`], so a slow hook
    /// delays only its own observability, never the loop) and turns each captured
    /// aggregate into user-visible inline notices. Errors are already logged by
    /// `dispatch`; hooks fired here stay observe-only, so any `decision` they
    /// return is deliberately not honored.
    async fn settle_fired_hooks(&self, session_id: &str) -> Vec<Message> {
        self.hooks_manager
            .settle_fired(session_id, crate::hooks::FIRE_JOIN_BUDGET)
            .await
            .into_iter()
            .flat_map(|outcome| {
                let event = outcome.event;
                outcome
                    .aggregate
                    .system_messages
                    .into_iter()
                    .map(move |msg| {
                        debug!("hooks: surfacing {event} systemMessage");
                        Message::assistant()
                            .with_system_notification(SystemNotificationType::InlineMessage, msg)
                            .user_only()
                    })
            })
            .collect()
    }

    /// BR-12: move auto-compaction off the user-visible critical path.
    ///
    /// Called at the turn boundary — after [`Self::record_turn_usage`] has
    /// written this turn's provider-reported token count and the reply loop has
    /// finished — so that if the session is now over the compaction threshold the
    /// (multi-second) summarization LLM round-trip runs in a detached
    /// `tokio::spawn` *between* turns instead of stalling the start of the next
    /// turn. The next turn then starts from an already-compacted history.
    ///
    /// The synchronous compaction at the top of `reply()` stays as the fallback:
    /// it fires when this background swap hasn't landed yet (a huge single turn, a
    /// fast follow-up message, or a failed task), so a session can never overflow
    /// even if eager compaction lags. That is the "keep a synchronous fallback"
    /// phasing BR-12 calls for; a later phase can lower the synchronous path to a
    /// 95%-budget no-LLM hard-drop floor.
    ///
    /// Idempotent per session: a second call while a compaction is in flight for
    /// the same session is a no-op, so the loop can call this freely.
    pub(super) fn maybe_spawn_eager_compaction(
        &self,
        session_config: &SessionConfig,
        working_dir: &std::path::Path,
    ) {
        if !crate::context_mgmt::eager_compaction_enabled() {
            return;
        }

        // At most one background compaction per session. `insert` returns false
        // when the id was already present (a task is running) — bail then.
        {
            let mut in_flight = self
                .eager_compactions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !in_flight.insert(session_config.id.clone()) {
                return;
            }
        }

        let provider = match self.provider.try_lock() {
            Ok(guard) => guard.clone(),
            // Provider is momentarily locked; skip — the synchronous fallback
            // still covers this session next turn.
            Err(_) => None,
        };
        let Some(provider) = provider else {
            self.clear_eager_compaction(&session_config.id);
            return;
        };
        // Issue #56 Gate B'. This is the ONE bypass of [`Agent::provider`] that
        // is inside a path Gate B' names as covered: compaction summarisation
        // reads the entire transcript, and its background half takes the
        // binding straight off the `SharedProvider` because it must not block
        // on the lock. The predicate is therefore asserted here by hand rather
        // than inherited from the accessor.
        //
        // A refusal takes the same exit as a momentarily-locked provider,
        // which is the honest outcome: eager compaction is a best-effort
        // optimisation, and the synchronous fallback at the top of the next
        // `reply` runs AFTER Gate B has repaired or refused the session, so
        // skipping here loses nothing but a round-trip.
        //
        // DR-15's master opt-out. Read directly: eager compaction is not a tool
        // call and has no admitted capability to inherit.
        if crate::privacy::privacy_tiers_enabled()
            && !crate::privacy::bind_allowed(provider.tier(), self.cached_classification.load())
        {
            tracing::warn!(
                session_id = session_config.id,
                provider = provider.get_name(),
                "skipping eager compaction: the bound provider does not satisfy this session's classification"
            );
            self.clear_eager_compaction(&session_config.id);
            return;
        }

        let session_manager = self.config.session_manager.clone();
        let hooks_manager = Arc::clone(&self.hooks_manager);
        let in_flight = Arc::clone(&self.eager_compactions);
        let session_config = session_config.clone();
        let working_dir = working_dir.to_path_buf();
        let threshold = Config::global()
            .get_param::<f64>("BIOROUTER_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
        let session_id = session_config.id.clone();

        tokio::spawn(async move {
            // Remove the in-flight marker no matter how the task exits.
            let _guard = EagerCompactionGuard {
                session_id: session_id.clone(),
                in_flight,
            };

            // Fire PreCompact only when compaction actually proceeds (the routine
            // calls this back after its threshold check passes) — never on a turn
            // that ended under budget.
            let precompact_hooks = Arc::clone(&hooks_manager);
            let precompact_id = session_id.clone();
            let precompact_dir = working_dir.clone();
            let on_before_compact = move || {
                fire_compaction_hook_on(
                    &precompact_hooks,
                    crate::hooks::HookEvent::PreCompact,
                    &precompact_id,
                    &precompact_dir,
                    "auto",
                    Some("eager"),
                );
            };

            match crate::context_mgmt::run_eager_compaction(
                provider,
                session_manager,
                session_config,
                threshold,
                on_before_compact,
            )
            .await
            {
                Ok(crate::context_mgmt::EagerCompactionOutcome::Swapped) => {
                    info!("BR-12: eager compaction swapped in for session {session_id}");
                    fire_compaction_hook_on(
                        &hooks_manager,
                        crate::hooks::HookEvent::PostCompact,
                        &session_id,
                        &working_dir,
                        "auto",
                        Some("eager"),
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("BR-12: eager compaction failed for session {session_id}: {e}");
                }
            }
        });
    }

    /// Clear the in-flight marker for a session's eager compaction. Used on the
    /// early-return path in [`Self::maybe_spawn_eager_compaction`] before any task
    /// was spawned (the spawned task clears it via [`EagerCompactionGuard`]).
    fn clear_eager_compaction(&self, session_id: &str) {
        self.eager_compactions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
    }

    async fn handle_approved_and_denied_tools(
        &self,
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        session: &Session,
        inspection_results: &[InspectionResult],
    ) -> Result<Vec<(String, ToolStream)>> {
        let mut tool_futures: Vec<(String, ToolStream)> = Vec::new();

        // Handle pre-approved and read-only tools
        for request in &permission_check_result.approved {
            if let Ok(tool_call) = request.tool_call.clone() {
                let (req_id, tool_result) = self
                    .dispatch_tool_call(
                        tool_call,
                        request.id.clone(),
                        cancel_token.clone(),
                        session,
                    )
                    .await;

                tool_futures.push((
                    req_id,
                    match tool_result {
                        Ok(result) => tool_stream(
                            result
                                .notification_stream
                                .unwrap_or_else(|| Box::new(stream::empty())),
                            result.result,
                        ),
                        Err(e) => {
                            tool_stream(Box::new(stream::empty()), futures::future::ready(Err(e)))
                        }
                    },
                ));
            }
        }

        Self::handle_denied_tools(
            permission_check_result,
            request_to_response_map,
            inspection_results,
        )
        .await;
        Ok(tool_futures)
    }

    async fn handle_denied_tools(
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        inspection_results: &[InspectionResult],
    ) {
        for request in &permission_check_result.denied {
            if let Some(response_msg) = request_to_response_map.get(&request.id) {
                // When an inspector denied this call, tell the model why so it
                // can adjust instead of blindly retrying. The always-on
                // catastrophic-command block (security inspector) and hook denials
                // carry a reason; surface it verbatim / with context.
                let deny_reason = inspection_results.iter().find(|result| {
                    result.tool_request_id == request.id
                        && result.action == InspectionAction::Deny
                        && !result.reason.trim().is_empty()
                });
                let response_text = match deny_reason {
                    Some(result)
                        if result.inspector_name
                            == crate::hooks::inspector::HOOK_INSPECTOR_NAME =>
                    {
                        format!("{DECLINED_RESPONSE}\n\nHook feedback: {}", result.reason)
                    }
                    // Non-bypassable safety block: the user did not decline, the
                    // command is refused outright, so return the reason directly.
                    Some(result) if result.inspector_name == "security" => result.reason.clone(),
                    // BR-29/BR-31: a loop guard tripped — the call repeated
                    // itself, or the tool has been failing the same way over and
                    // over. The user did not decline anything; telling the model
                    // they did (the old DECLINED_RESPONSE) is actively misleading
                    // and leaves it unable to diagnose the stop. Return the real
                    // reason.
                    Some(result)
                        if result.inspector_name
                            == crate::tool_monitor::REPETITION_INSPECTOR_NAME =>
                    {
                        result.reason.clone()
                    }
                    // #63: a cross-session memory shape Biorouter refuses (the
                    // whole-store global read). Same reasoning as the loop
                    // guards above — the user declined nothing, and the reason
                    // is the only thing that tells the model the itemised call
                    // still works. `DECLINED_RESPONSE` here would be both untrue
                    // and unactionable, and would read as the feature being off.
                    Some(result)
                        if result.inspector_name
                            == crate::security::global_memory::GLOBAL_MEMORY_INSPECTOR_NAME =>
                    {
                        result.reason.clone()
                    }
                    _ => DECLINED_RESPONSE.to_string(),
                };
                let mut response = response_msg.lock().await;
                *response = response.clone().with_tool_response_with_metadata(
                    request.id.clone(),
                    Ok(CallToolResult {
                        content: vec![rmcp::model::Content::text(response_text)],
                        structured_content: None,
                        is_error: Some(true),
                        meta: None,
                    }),
                    request.metadata.as_ref(),
                );
            }
        }
    }

    /// Get a reference count clone to the provider
    pub async fn provider(&self) -> Result<Arc<dyn Provider>, anyhow::Error> {
        let provider = self
            .bound_provider_unchecked()
            .await
            .ok_or_else(|| anyhow!("Provider not set"))?;
        // Issue #56 Gate B'. The non-`reply` completion paths — session
        // auto-naming (`maybe_rename_session` -> `complete_fast`), compaction
        // summarisation (`context_mgmt::compact_messages`) and the stall judge
        // (`agents::stall`) — all read the entire transcript and none of them
        // passes Gate B, because none of them goes through `reply`. They do all
        // take their provider from here.
        //
        // The classification is the cached one rather than a fresh row read:
        // this accessor is called several times per turn from inside the reply
        // loop, and a database round-trip on each would be a real cost for a
        // value `reply` has just read. See [`CachedClassification`] for why the
        // cache is sound and why it fails closed.
        //
        // DR-15's master opt-out. Read directly: this accessor serves the
        // non-`reply` completion paths, none of which is a tool call with an
        // admitted capability to inherit.
        let cached = self.cached_classification.load();
        if crate::privacy::privacy_tiers_enabled()
            && !crate::privacy::bind_allowed(provider.tier(), cached)
        {
            return Err(PrivacyRefusal::PublicModelOnPrivateSession {
                session_id: self.cached_classification.session_id(),
                provider: provider.get_name().to_string(),
            }
            .into());
        }
        Ok(provider)
    }

    /// The bound provider WITHOUT Gate B'.
    ///
    /// Gate B itself must read the raw binding: asking [`Agent::provider`]
    /// there would consult the very cached classification the gate is about to
    /// replace, and would report "no provider" for the mismatch the gate exists
    /// to repair.
    ///
    /// ⚠ Deliberately not `pub`, but do NOT read that as "the only way past
    /// Gate B'" — an earlier draft of this comment claimed exactly that and it
    /// was false. `SharedProvider` is a clonable `Arc<Mutex<..>>`; three
    /// production sites hold one and go straight to the binding. See
    /// [`CachedClassification`] for the enumeration and which of them are
    /// asserted.
    pub(super) async fn bound_provider_unchecked(&self) -> Option<Arc<dyn Provider>> {
        self.provider.lock().await.clone()
    }

    /// Issue #56 Gate B, the repair arm: re-bind the provider the session ROW
    /// records, when the live agent is holding one the row's classification
    /// does not admit.
    ///
    /// Returns `Ok(true)` when the row's own provider was constructed, found to
    /// satisfy the classification, and swapped in; `Ok(false)` when the row
    /// names nothing usable, or names something still public. Errors from the
    /// provider factory are `Err` and are treated by the caller exactly like
    /// `Ok(false)` — a refusal — because a repair that did not happen must
    /// never read as a repair that did.
    ///
    /// This is the COMMON case, not an exotic one: LRU rehydration,
    /// `restore_provider_from_session`'s `Config::global()` fallback, a legacy
    /// row written before #56, and every ratchet that commits after a legal
    /// bind all leave exactly this state.
    ///
    /// ⚠ The swap is in-memory only and does NOT go through `update_provider`.
    /// There is nothing to persist — the row already names this provider, byte
    /// for byte — and re-entering Gate A to write a row's own value back to
    /// itself would put a database write on the front of every turn of a
    /// rehydrated session.
    async fn rebind_from_row(&self, row: &Session) -> Result<bool> {
        let Some(provider_name) = row.provider_name.clone() else {
            return Ok(false);
        };
        // A row with a `provider_name` but no `model_config` is a legacy row:
        // `update_provider` has always written both in one statement. Fall back
        // the same way `restore_provider_from_session` does rather than
        // refusing a session that is perfectly repairable.
        let model_config = match row.model_config.clone() {
            Some(model_config) => model_config,
            None => {
                let model_name = Config::global()
                    .get_biorouter_model()
                    .map_err(|e| anyhow!("no model recorded for this session: {e}"))?;
                crate::model::ModelConfig::new(&model_name)?
            }
        };

        #[cfg(test)]
        let provider = match seams::rebind_override(&row.id, &provider_name) {
            Some(provider) => provider,
            None => crate::providers::create(&provider_name, model_config).await?,
        };
        #[cfg(not(test))]
        let provider = crate::providers::create(&provider_name, model_config).await?;

        if !crate::privacy::bind_allowed(provider.tier(), row.privacy_tier) {
            return Ok(false);
        }
        *self.provider.lock().await = Some(provider);
        Ok(true)
    }

    /// BR-63: set the session's sticky reasoning effort (`/effort <level>`).
    pub async fn set_reasoning_effort(&self, session_id: &str, effort: ReasoningEffort) {
        self.efforts.set(session_id, effort).await;
    }

    /// The session's sticky reasoning effort, if `/effort` set one.
    pub async fn reasoning_effort(&self, session_id: &str) -> ReasoningEffort {
        self.efforts.get(session_id).await.unwrap_or_default()
    }

    /// Resolve the effort for one turn: an explicit per-turn effort on the
    /// request (the GUI composer toggle) wins over the session's sticky
    /// `/effort`, which in turn wins over the default (`Normal`, a no-op).
    async fn resolve_effort(&self, session_config: &SessionConfig) -> ReasoningEffort {
        match session_config.reasoning_effort {
            Some(effort) => effort,
            None => self.reasoning_effort(&session_config.id).await,
        }
    }

    /// The effort-stamped provider to run this turn's completions through, or
    /// `None` when the turn should just use the session's provider as it always
    /// has (the default effort, or a provider the effort can't be applied to).
    ///
    /// `quick`/`deep` re-stamp the model config with the effort and rebuild the
    /// provider around it, once per reply — the streaming path reads its model
    /// config off the provider, so there is nowhere else to inject a per-turn
    /// config.
    ///
    /// Failure is not fatal: an unreconstructible provider (a lead/worker
    /// composite, a provider whose registry entry is gone) falls back to the
    /// session's provider, which still gets the effort's exploration caps. That
    /// is the "degrade gracefully" the proposal asks for.
    async fn provider_with_effort(
        &self,
        effort: ReasoningEffort,
    ) -> Result<Option<Arc<dyn Provider>>> {
        if effort.is_default() {
            return Ok(None);
        }
        let provider = self.provider().await?;
        if provider.as_lead_worker().is_some() {
            return Ok(None);
        }

        let model_config = effort.apply_to_model(provider.get_model_config());
        match crate::providers::create(provider.get_name(), model_config).await {
            Ok(rebuilt) => Ok(Some(rebuilt)),
            Err(e) => {
                warn!(
                    "Reasoning effort '{}' not applied to provider '{}' ({}); \
                     falling back to the session provider (exploration caps still apply)",
                    effort.as_str(),
                    provider.get_name(),
                    e
                );
                Ok(None)
            }
        }
    }

    /// Check if a tool is a frontend tool
    pub async fn is_frontend_tool(&self, name: &str) -> bool {
        self.frontend_tools.lock().await.contains_key(name)
    }

    /// Get a reference to a frontend tool
    pub async fn get_frontend_tool(&self, name: &str) -> Option<FrontendTool> {
        self.frontend_tools.lock().await.get(name).cloned()
    }

    pub async fn add_final_output_tool(&self, response: Response) {
        let mut final_output_tool = self.final_output_tool.lock().await;
        let created_final_output_tool = FinalOutputTool::new(response);
        let final_output_system_prompt = created_final_output_tool.system_prompt();
        *final_output_tool = Some(created_final_output_tool);
        self.extend_system_prompt(final_output_system_prompt).await;
    }

    pub async fn add_sub_workflows(&self, sub_workflows_to_add: Vec<SubWorkflow>) {
        let mut sub_workflows = self.sub_workflows.lock().await;
        for sr in sub_workflows_to_add {
            sub_workflows.insert(sr.name.clone(), sr);
        }
    }

    pub async fn apply_workflow_components(
        &self,
        sub_workflows: Option<Vec<SubWorkflow>>,
        response: Option<Response>,
        include_final_output: bool,
    ) {
        if let Some(sub_workflows) = sub_workflows {
            self.add_sub_workflows(sub_workflows).await;
        }

        if include_final_output {
            if let Some(response) = response {
                self.add_final_output_tool(response).await;
            }
        }
    }

    /// Establish this turn's tool bridge, when the bound provider is one that needs
    /// one.
    ///
    /// Only `claude_code` and `codex` do: they run their own agent loop in a child
    /// process, so the only way they can reach Biorouter's tools is by calling back
    /// in over MCP. Every other provider receives its tools in the request and
    /// needs no grant, and issuing one anyway would leave a live capability on
    /// every turn in the process.
    ///
    /// Returns `None` when there is no bridge to offer — a CLI process with no HTTP
    /// server has nothing for a child to connect to. The providers read that as
    /// "run with no tools", which is the right degradation: an answer without tools
    /// beats a failed turn.
    ///
    /// The returned lease revokes the grant when dropped, so it is bound in the
    /// caller for the duration of the provider call and no longer.
    async fn issue_tool_bridge(
        &self,
        session: &Session,
        conversation: &Conversation,
        tools: &[Tool],
    ) -> Option<coding_agent_bridge::BridgeLease> {
        let name = {
            let guard = self.provider.lock().await;
            guard.as_ref().map(|p| p.get_name().to_string())?
        };
        if !coding_agent_bridge::provider_uses_bridge(&name) {
            return None;
        }

        // Sampled once, here, and carried in the grant. A bridged call is a call,
        // and `CallCapability` exists so a call's privacy capability is fixed
        // before it runs rather than re-read while it runs.
        let capability = crate::privacy::CallCapability::sample(&self.provider).await;

        // Advertise only what the bridge can actually execute.
        //
        // `tools` is not just the extension surface — `prepare_tools` deliberately
        // includes the platform, frontend, subagent and final-output tools so the
        // risk registry grades them. Those are dispatched by the branches at the
        // top of `dispatch_tool_call`, NOT by the `ExtensionManager` the grant
        // holds, so offering them over the bridge would advertise tools that then
        // fail to resolve — the child would burn a turn calling something that was
        // never going to work, and the failure would look like a broken tool
        // rather than a missing one.
        //
        // Filtering here rather than in the grant because `is_frontend_tool` is
        // per-agent state the grant has no access to.
        let mut bridged = Vec::with_capacity(tools.len());
        for tool in tools {
            let name = tool.name.as_ref();
            let dispatched_elsewhere = is_spawn_tool_call(name)
                || name == crate::agents::platform_tools::PLATFORM_MANAGE_SCHEDULE_TOOL_NAME
                || name == crate::agents::platform_tools::PLATFORM_INGEST_CONVERSATION_TOOL_NAME
                || name == crate::agents::platform_tools::PLATFORM_READ_SESSION_BLOB_TOOL_NAME
                || name == crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME
                || self.is_frontend_tool(name).await;
            if !dispatched_elsewhere {
                bridged.push(tool.clone());
            }
        }

        coding_agent_bridge::issue(coding_agent_bridge::BridgeGrant::new(
            session.clone(),
            self.config.biorouter_mode,
            Arc::clone(&self.extension_manager),
            Arc::clone(&self.tool_inspection_manager),
            capability,
            bridged,
            conversation.clone(),
        ))
    }

    /// Dispatch a tool call that arrived from a **bridged coding agent** rather
    /// than from this agent's own model.
    ///
    /// `claude_code` and `codex` run their own loop in a child process, and
    /// Biorouter hands them its extension tools over MCP so that a tool the child
    /// decides to call is still *executed here*. This is the entry point for that,
    /// and it exists rather than reusing a route because the two obvious options
    /// are both wrong:
    ///
    /// * `ExtensionManager::dispatch_tool_call` directly, as `POST
    ///   /agent/call_tool` does, skips every [`ToolInspector`] — so no PreToolUse
    ///   hook, no sensitive-operation check, and no permission mode.
    /// * [`Self::dispatch_tool_call`] alone samples the capability correctly (so
    ///   Gate C, cross-affiliation, the vault and the secret guard all apply) but
    ///   still runs no inspectors, because in a normal turn those run earlier, in
    ///   the reply loop.
    ///
    /// So this runs the inspectors first, on a synthetic one-element request batch,
    /// exactly as the reply loop would, and only then dispatches.
    ///
    /// A call the permission inspector puts in `needs_approval` is **refused**, not
    /// parked. The child is blocked on an HTTP response with no channel through
    /// which a human could answer, so waiting would deadlock the turn; refusing
    /// with an explanation lets the child's model report back and the user retry
    /// deliberately. Refusing is also the fail-safe direction.
    pub async fn dispatch_bridged_tool_call(
        &self,
        tool_call: CallToolRequestParams,
        session: &Session,
        conversation: &Conversation,
        cancellation_token: Option<CancellationToken>,
    ) -> Result<ToolCallResult, ErrorData> {
        let name = tool_call.name.to_string();
        let request_id = uuid::Uuid::new_v4().to_string();
        let mut requests = vec![ToolRequest {
            id: request_id.clone(),
            tool_call: Ok(tool_call.clone()),
            metadata: None,
            tool_meta: None,
        }];

        let mode = self.config.biorouter_mode;
        let inspections = self
            .tool_inspection_manager
            .inspect_tools(&requests, conversation.messages(), mode, session)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("could not inspect `{name}`: {e}"),
                    None,
                )
            })?;

        // No permission inspector configured means nothing decided, which must not
        // read as "approved".
        let decision = self
            .tool_inspection_manager
            .process_inspection_results_with_permission_inspector(&requests, &inspections)
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("no permission decision was reached for `{name}`"),
                    None,
                )
            })?;

        if !decision.denied.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("`{name}` was denied by Biorouter's tool policy."),
                None,
            ));
        }
        if !decision.needs_approval.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!(
                    "`{name}` needs a person's approval, and this session's coding agent has no \
                     way to ask for one mid-turn. Tell the user what you wanted to run and why, \
                     and let them approve it."
                ),
                None,
            ));
        }
        if decision.approved.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("`{name}` was not approved."),
                None,
            ));
        }

        // Dispatch the request the inspectors actually cleared, not the original —
        // a PreToolUse hook may have rewritten its arguments, and those rewrites
        // were re-validated inside the inspection pass.
        let cleared = requests
            .pop()
            .and_then(|r| r.tool_call.ok())
            .unwrap_or(tool_call);

        let (_, result) = self
            .dispatch_tool_call(cleared, request_id, cancellation_token, session)
            .await;
        result
    }

    /// Dispatch a single tool call to the appropriate client
    #[instrument(skip(self, tool_call, request_id), fields(input, output))]
    #[allow(clippy::too_many_lines)]
    pub async fn dispatch_tool_call(
        &self,
        mut tool_call: CallToolRequestParams,
        request_id: String,
        cancellation_token: Option<CancellationToken>,
        session: &Session,
    ) -> (String, Result<ToolCallResult, ErrorData>) {
        // BR-71 §5: no workspace control, and no nesting, inside a delegation tree.
        if is_workspace_tool_refused_for(session.session_type, tool_call.name.as_ref()) {
            let message = if is_spawn_tool_call(tool_call.name.as_ref()) {
                "Subagents cannot create other subagents. Do the work yourself, or \
                 report back to your parent so it can delegate."
            } else {
                "Subagents cannot use workspace tools."
            };
            return (
                request_id,
                Err(ErrorData::new(
                    ErrorCode::INVALID_REQUEST,
                    message.to_string(),
                    None,
                )),
            );
        }

        if tool_call.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            // Issue #56: the schedule branch returns from `dispatch_tool_call`
            // BEFORE the agent loop's own sample below, so `platform__manage_schedule`
            // had no capability in scope at all — and two of its actions read
            // another chat's content. Sampled here, on the same provider mutex and
            // for the same reason: the value the call is granted must be fixed
            // before the call runs, not re-read while it runs.
            let cap = crate::privacy::CallCapability::sample(&self.provider).await;
            let result = self
                .handle_schedule_management(arguments, request_id.clone(), &session.id, cap)
                .await;
            let wrapped_result = result.map(|content| CallToolResult {
                content,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            });
            return (request_id, Ok(ToolCallResult::from(wrapped_result)));
        }

        if tool_call.name == PLATFORM_INGEST_CONVERSATION_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let result = self.handle_ingest_conversation(arguments, session).await;
            let wrapped_result = result.map(|content| CallToolResult {
                content,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            });
            return (request_id, Ok(ToolCallResult::from(wrapped_result)));
        }

        // BR-7: read back a tool result that was externalized out of the
        // conversation. Reads the session store, so it never touches the
        // extension manager's dispatch path.
        if tool_call.name == PLATFORM_READ_SESSION_BLOB_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let result = self.handle_read_session_blob(arguments, session).await;
            let wrapped_result = result.map(|content| CallToolResult {
                content,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            });
            return (request_id, Ok(ToolCallResult::from(wrapped_result)));
        }

        if tool_call.name == FINAL_OUTPUT_TOOL_NAME {
            return if let Some(final_output_tool) = self.final_output_tool.lock().await.as_mut() {
                let result = final_output_tool.execute_tool_call(tool_call.clone()).await;
                (request_id, Ok(result))
            } else {
                (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        "Final output tool not defined".to_string(),
                        None,
                    )),
                )
            };
        }

        debug!("WAITING_TOOL_START: {}", tool_call.name);
        let result: ToolCallResult = if is_spawn_tool_call(tool_call.name.as_ref()) {
            // Same gate `subagents_enabled` applies when advertising. The
            // extension advertises the tool; this is what stops a model that
            // remembers the name from spawning in a session where delegation is
            // off.
            if !self.subagents_enabled(&session.id).await {
                return (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INVALID_REQUEST,
                        "Subagent delegation is not available in this session".to_string(),
                        None,
                    )),
                );
            }
            // …and the PER-SESSION grant, which the mode gate above does not
            // cover. Intercepting before `ExtensionManager::dispatch_tool_call`
            // means its `is_tool_available` check never runs for this name, so a
            // session whose `workspace` entry was deliberately restricted — say
            // `available_tools: ["workspace_list"]` — could still spawn through
            // the BARE `subagent`. Re-checking here is what makes "enforced in
            // both places" true for the spawn tool too, not just for
            // `workspace_*`.
            if !self
                .extension_manager
                .is_extension_tool_available(Self::SPAWN_EXTENSION, SUBAGENT_TOOL_NAME)
                .await
            {
                return (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::RESOURCE_NOT_FOUND,
                        "Tool 'subagent' is not available for extension 'workspace'".to_string(),
                        None,
                    )),
                );
            }
            let provider = match self.provider().await {
                Ok(p) => p,
                Err(_) => {
                    return (
                        request_id,
                        Err(ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            "Provider is required".to_string(),
                            None,
                        )),
                    );
                }
            };

            let extensions = self.get_extension_configs().await;
            let task_config =
                TaskConfig::new(provider, &session.id, &session.working_dir, extensions);
            let sub_workflows = self.sub_workflows.lock().await.clone();

            let arguments = tool_call
                .arguments
                .clone()
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));

            handle_subagent_tool(
                &self.config,
                arguments,
                task_config,
                sub_workflows,
                session.working_dir.clone(),
                cancellation_token,
            )
        } else if self.is_frontend_tool(&tool_call.name).await {
            // For frontend tools, return an error indicating we need frontend execution
            ToolCallResult::from(Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Frontend tool execution required".to_string(),
                None,
            )))
        } else {
            // BRSDK encryption: resolve {{vault:NAME}} secrets ONLY here — on the
            // leaf MCP-dispatch path, after the model produced the call and right
            // before the tool runs. Deliberately NOT applied to the subagent /
            // frontend / final_output / schedule branches above, whose arguments
            // are re-consumed by an LLM, returned to the browser, or persisted — a
            // resolved secret there would leak. No-op unless a vault is installed.
            self.apply_vault(&mut tool_call).await;

            // Issue #56: THE sample for the agent loop's tool calls. Taken here,
            // once, and carried the rest of the way — every barrier downstream
            // reads this value rather than the provider mutex, so a swap that
            // lands while the call sits behind the dispatch semaphore cannot
            // change what the call already got permission to do.
            let cap = crate::privacy::CallCapability::sample(&self.provider).await;

            // Clone the result to ensure no references to extension_manager are returned
            let result = self
                .extension_manager
                .dispatch_tool_call(
                    &session.id,
                    tool_call.clone(),
                    cap,
                    cancellation_token.unwrap_or_default(),
                )
                .await;
            result.unwrap_or_else(|e| {
                // Try to downcast to ErrorData to avoid double wrapping
                let error_data = e.downcast::<ErrorData>().unwrap_or_else(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                });
                ToolCallResult::from(Err(error_data))
            })
        };

        debug!("WAITING_TOOL_END: {}", tool_call.name);

        // BR-6: the large-response handler needs the session working dir so an
        // oversized result is offloaded to a handle the model's file/shell
        // tools can actually reach (not a bare temp path outside the sandbox).
        let large_response_ctx = super::large_response_handler::LargeResponseContext {
            session_id: session.id.clone(),
            working_dir: session.working_dir.clone(),
            tool_name: tool_call.name.to_string(),
        };
        let inner = result.result;

        // BR-58: the returned future is what `select_all` drives concurrently, so
        // this is the choke point that must bound total tool parallelism and
        // serialize overlapping write paths. The guard is acquired *inside* the
        // future (not eagerly) so parking on it does not stall the dispatch loop,
        // and is held across the whole execution + post-processing.
        //
        // The subagent tool is deliberately excluded: it recursively runs its own
        // agent loop whose leaf tools contend for this *same* global semaphore, so
        // a subagent wrapper holding a permit while its inner tools wait for one
        // would deadlock. Subagents already have their own `SUBAGENT_SEMAPHORE`;
        // their leaf tools are still bounded here (permit acquired before any
        // path lock, so a lock holder always makes progress — no deadlock).
        let dispatch_args = tool_call.arguments.clone();
        // BR-71: the spawn exclusion above now has to cover BOTH name forms —
        // the tool is advertised as `workspace__subagent`, and the bare name
        // still arrives from prefix-stripping models.
        //
        // BR-71 also adds two more wrappers with exactly that shape. Both PARK on
        // work performed elsewhere, for up to 600 s:
        //
        //   * `workspace_watch` — waits on other sessions' bus events (up to 32
        //     ids, `timeout_s` clamped to 600). Eight concurrent watches take
        //     all eight permits and stall every other tool call in the daemon,
        //     including the user's own foreground conversation, for ten minutes.
        //   * `workspace_send_prompt` with `mode:"turn", wait:"final_message"` —
        //     the true deadlock: it holds a permit while waiting for the TARGET
        //     session's detached turn to finish, and that turn's own tool calls
        //     contend for the same eight permits. At saturation nothing can
        //     complete until the timeout fires.
        //
        // A parking wrapper does no work of its own, so exempting it does not
        // widen the concurrency this semaphore exists to bound.
        let bound_dispatch = !is_spawn_tool_call(tool_call.name.as_ref())
            && !is_parking_workspace_tool(tool_call.name.as_ref());
        // Stage 0 instrumentation: the existing WAITING_TOOL_START/END pair
        // above brackets only dispatch *setup* — the future below is returned
        // un-awaited and driven later by `select_all`, so those markers close
        // in microseconds while the tool runs for seconds. Any claim about
        // tools running serially has to be measured HERE, inside the future,
        // around the actual await. This is the gate for the H5/H6 work.
        let exec_tool_name = tool_call.name.to_string();
        let exec_request_id = request_id.clone();

        (
            request_id,
            Ok(ToolCallResult {
                notification_stream: result.notification_stream,
                result: Box::new(Box::pin(async move {
                    // Issue #56: the far side of the permit-to-execution gap.
                    // A test parks here to prove that a provider swap landing
                    // between admission and execution does NOT change what this
                    // call may do. Keyed on this call's session and tool name so
                    // it can only catch the dispatch that armed it. Compiled out
                    // entirely in a non-test build.
                    #[cfg(test)]
                    seams::dispatch_queue_hold(
                        &large_response_ctx.session_id,
                        &large_response_ctx.tool_name,
                    )
                    .await;
                    let _dispatch_guard = if bound_dispatch {
                        Some(
                            super::tool_dispatch_limits::acquire(
                                &large_response_ctx.tool_name,
                                dispatch_args.as_ref(),
                                &large_response_ctx.working_dir,
                            )
                            .await,
                        )
                    } else {
                        None
                    };
                    // Timed after the dispatch guard is acquired, so this span
                    // is execution only and excludes time parked on the
                    // concurrency semaphore.
                    let exec_started = std::time::Instant::now();
                    debug!(
                        name = %exec_tool_name,
                        id = %exec_request_id,
                        "TOOL_EXEC_START"
                    );
                    let inner_result = inner.await;
                    let dur_ms = exec_started.elapsed().as_millis() as u64;
                    debug!(
                        name = %exec_tool_name,
                        id = %exec_request_id,
                        dur_ms,
                        "TOOL_EXEC_END"
                    );
                    // Single, always-on, structured record of every tool result:
                    // one info line per tool call keyed `target: "tool_result"`,
                    // carrying tool name, ok/error, and (on failure) the error
                    // text. This is the one place every dispatched tool call's
                    // outcome is inspectable regardless of extension, error
                    // surface (a hard `Err(ErrorData)` OR an `Ok` result flagged
                    // `is_error`), or interface (GUI/CLI). Kept cheap: the error
                    // string is only materialised on the failure path. See
                    // docs/agent-loop/tool-routing.md ("Tool-result logging").
                    match &inner_result {
                        Err(e) => tracing::info!(
                            target: "tool_result",
                            tool = %exec_tool_name,
                            id = %exec_request_id,
                            ok = false,
                            dur_ms,
                            error = %e.message,
                            "tool call failed",
                        ),
                        Ok(r) if r.is_error == Some(true) => {
                            let error_text = r
                                .content
                                .iter()
                                .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                                .collect::<Vec<_>>()
                                .join(" ");
                            tracing::info!(
                                target: "tool_result",
                                tool = %exec_tool_name,
                                id = %exec_request_id,
                                ok = false,
                                dur_ms,
                                error = %error_text,
                                "tool call returned error result",
                            );
                        }
                        Ok(_) => tracing::info!(
                            target: "tool_result",
                            tool = %exec_tool_name,
                            id = %exec_request_id,
                            ok = true,
                            dur_ms,
                            "tool call ok",
                        ),
                    }
                    super::large_response_handler::process_tool_response(
                        inner_result,
                        &large_response_ctx,
                    )
                    .await
                })),
            }),
        )
    }

    /// Extension configs that may be recorded as the user's session
    /// configuration.
    ///
    /// An extension this agent injected for ITSELF (`ensure_spawn_extension`)
    /// is a derived per-turn consequence of `subagents_enabled`, not a user
    /// decision, and must never reach the session row. That exclusion is not
    /// applied here: it belongs to the extension entry itself
    /// ([`ExtensionOrigin`]), so it is decided in the same critical section as
    /// the load it describes and cannot be observed out of step with it. This
    /// method survives as the name for the *intent* — both persist paths go
    /// through it, because both snapshot every loaded extension rather than the
    /// one being changed, and a future reader needs to see that they share one
    /// definition of "persistable".
    pub(super) async fn persistable_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extension_manager.get_extension_configs().await
    }

    /// Save current extension state to session metadata
    /// Should be called after any extension add/remove operation
    pub async fn save_extension_state(&self, session: &SessionConfig) -> Result<()> {
        self.write_enabled_extensions(&session.id).await
    }

    /// Save current extension state to session by session_id
    pub async fn persist_extension_state(&self, session_id: &str) -> Result<()> {
        self.write_enabled_extensions(session_id).await
    }

    /// Record the session's enabled extensions, touching NO other key of
    /// `extension_data`.
    ///
    /// Both callers above used to read the session, mutate the whole
    /// [`ExtensionData`] object in a local copy, and write that whole object
    /// back — two statements, so a writer of a *different* key that committed
    /// in between (`todo.v1`, `goal.v0`, `run_state.*`, `workspace_skills.v1`
    /// all share this one JSON column) was silently erased by the second one.
    /// `SessionManager::update_extension_state` exists for exactly this: it
    /// does the read and the write inside one transaction that opens with a
    /// write, so writers serialize and no merge basis can be stale.
    ///
    /// Both of these paths are tool-triggered — the GUI extension toggle and
    /// `workspace_set_tools` on one, the reply loop's `manage_extensions` save
    /// on the other — and tool calls overlap by construction, so the window was
    /// reachable rather than theoretical.
    async fn write_enabled_extensions(&self, session_id: &str) -> Result<()> {
        let extensions_state =
            EnabledExtensionsState::new(self.persistable_extension_configs().await);
        let value = extensions_state
            .to_value()
            .map_err(|e| anyhow!("Extension state serialization failed: {}", e))?;

        let written = self
            .config
            .session_manager
            .update_extension_state(
                session_id,
                EnabledExtensionsState::EXTENSION_NAME,
                EnabledExtensionsState::VERSION,
                move |_| Ok(value),
            )
            .await?;

        // `update_extension_state` reports a missing session as `Ok(None)`
        // rather than writing; the old `get_session` first line failed loudly
        // in that case and callers log on `Err`, so keep it loud.
        if written.is_none() {
            return Err(anyhow!(
                "cannot record extension state: no session {session_id}"
            ));
        }
        Ok(())
    }

    /// Load extensions from session into the agent
    /// Skips extensions that are already loaded
    pub async fn load_extensions_from_session(
        self: &Arc<Self>,
        session: &Session,
    ) -> Vec<ExtensionLoadResult> {
        // Bind extensions to the session's working directory so the shell tool
        // (and child-process extensions) run where the user is working. The GUI
        // folder picker persists the new dir and restarts the agent, which
        // re-enters this path, so the change takes effect on the next load.
        self.extension_manager
            .set_working_dir(session.working_dir.clone())
            .await;

        let session_extensions =
            EnabledExtensionsState::from_extension_data(&session.extension_data);
        let enabled_configs = match session_extensions {
            Some(state) => state.extensions,
            None => {
                tracing::warn!(
                    "No extensions found in session {}. This is unexpected.",
                    session.id
                );
                return vec![];
            }
        };

        let extension_futures = enabled_configs
            .into_iter()
            .map(|config| {
                let config_clone = config.clone();
                let agent_ref = self.clone();

                async move {
                    let name = config_clone.name().to_string();
                    let normalized_name = normalize(&name);

                    // BR-71 decision 21: "already loaded" must not include a
                    // copy THIS AGENT auto-injected. `ensure_spawn_extension`
                    // loads `workspace` with `available_tools: ["subagent"]`,
                    // and a turn can run before (or concurrently with) this
                    // load — at which point skipping would leave the session's
                    // own, explicitly configured full-surface entry
                    // permanently shadowed by the spawn-only one. Falling
                    // through instead is safe: an explicit config replaces an
                    // auto-injected entry of the same key.
                    //
                    // Presence and provenance are read together, so this cannot
                    // fall through on a stale "it was an injection a moment ago".
                    if agent_ref
                        .extension_manager
                        .extension_origin(&normalized_name)
                        .await
                        == Some(crate::agents::extension_manager::ExtensionOrigin::Explicit)
                    {
                        tracing::debug!("Extension {} already loaded, skipping", name);
                        return ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        };
                    }

                    match agent_ref.add_extension(config_clone).await {
                        Ok(_) => ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        },
                        Err(e) => {
                            let error_msg = e.to_string();
                            warn!("Failed to load extension {}: {}", name, error_msg);
                            ExtensionLoadResult {
                                name,
                                success: false,
                                error: Some(error_msg),
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        futures::future::join_all(extension_futures).await
    }

    /// Load an extension the user asked for.
    ///
    /// BR-71 decision 21: an explicit enable of a name the agent auto-injected
    /// REPLACES the injection rather than being swallowed by it. That rule is
    /// enforced by [`ExtensionManager::add_extension`], not here — the model's
    /// own `manage_extensions` calls the manager directly and never passes
    /// through this method, so a rule implemented at this level would simply
    /// not be on that path.
    pub async fn add_extension(&self, extension: ExtensionConfig) -> ExtensionResult<()> {
        match &extension {
            ExtensionConfig::Frontend {
                tools,
                instructions,
                ..
            } => {
                // For frontend tools, just store them in the frontend_tools map
                let mut frontend_tools = self.frontend_tools.lock().await;
                for tool in tools {
                    let frontend_tool = FrontendTool {
                        name: tool.name.to_string(),
                        tool: tool.clone(),
                    };
                    frontend_tools.insert(tool.name.to_string(), frontend_tool);
                }
                // Store instructions if provided, using "frontend" as the key
                let mut frontend_instructions = self.frontend_instructions.lock().await;
                if let Some(instructions) = instructions {
                    *frontend_instructions = Some(instructions.clone());
                } else {
                    // Default frontend instructions if none provided
                    *frontend_instructions = Some(
                        "The following tools are provided directly by the frontend and will be executed by the frontend when called.".to_string(),
                    );
                }
            }
            _ => {
                self.extension_manager
                    .add_extension(extension.clone())
                    .await?;
            }
        }

        Ok(())
    }

    /// Offer (or withhold) the generic `subagent` tool.
    ///
    /// Agent-Drafter apps with declared worker profiles call this with `false`, so
    /// `consult` is the ONE delegation mechanism. Two armed mechanisms is what let
    /// the main agent bypass the profiles the author declared.
    pub fn set_subagent_tool_enabled(&self, enabled: bool) {
        self.subagent_tool_enabled.store(enabled, Ordering::Relaxed);
    }

    pub async fn subagents_enabled(&self, session_id: &str) -> bool {
        // An app that delegates through `consult(agent: …)` must not ALSO be
        // offered the generic `subagent` tool — see `subagent_tool_enabled`.
        if !self.subagent_tool_enabled.load(Ordering::Relaxed) {
            return false;
        }
        if self.config.biorouter_mode != BioRouterMode::Auto {
            return false;
        }
        if self
            .provider()
            .await
            .map(|provider| provider.get_active_model_name().starts_with("gemini"))
            .unwrap_or(false)
        {
            return false;
        }
        let context = self.extension_manager.get_context();
        if matches!(
            context
                .session_manager
                .get_session(session_id, false)
                .await
                .ok()
                .map(|session| session.session_type),
            Some(SessionType::SubAgent)
        ) {
            return false;
        }
        // "Is anything loaded at all" — but NOT counting the extension this
        // gate's own answer causes to be loaded. `ensure_spawn_extension` puts
        // `workspace` in the map, so counting it would make one turn's `true`
        // the reason for the next turn's `true`: an agent that removed its last
        // real extension would keep delegating forever off a grant it derived
        // from itself. See `ExtensionManager::has_non_injected_extensions`.
        self.extension_manager.has_non_injected_extensions().await
    }

    /// Drop an injection this agent made, once the session state that justified
    /// it is gone. Only ever removes an [`ExtensionOrigin::AutoInjected`] entry
    /// — an explicitly enabled Workspace Control is the user's and stays.
    ///
    /// This is what makes the grant genuinely derived rather than sticky.
    /// Merely skipping the next injection changes nothing: the extension is
    /// already loaded and `get_prefixed_tools` reads the manager
    /// unconditionally, so the tool would keep being advertised — and remain
    /// dispatchable, since the spawn gate keys on `session_type`, never on
    /// `subagents_enabled`.
    ///
    /// One `Agent` can serve sessions that disagree about this (ACP shares an
    /// agent across all of its sessions), so an eligible and an ineligible
    /// session interleaving will load and unload the extension in turn. That is
    /// the price of a per-session answer from an agent-wide extension map, and
    /// it is paid in the right direction: each `list_tools` returns a list
    /// correct for the session that asked, and `workspace` is a cheap
    /// in-process platform extension.
    async fn revoke_spawn_extension(&self) {
        self.extension_manager
            .remove_if_auto_injected(Self::SPAWN_EXTENSION)
            .await;
    }

    /// The extension key the spawn tool is advertised under. Already canonical:
    /// `normalize(name_to_key("workspace")) == "workspace"`, so this is
    /// simultaneously the `PLATFORM_EXTENSIONS` registry key, the
    /// extension-manager map key, and the advertised tool prefix.
    pub(crate) const SPAWN_EXTENSION: &'static str = "workspace";

    /// Idempotently load the workspace extension for a session that may
    /// delegate. Never downgrades a user-enabled entry, and never claims one:
    /// both the "is it already there" decision and the provenance stamp are
    /// taken inside [`ExtensionManager::add_extension_auto_injected`], under the
    /// map's own lock, so an explicit enable racing this injection cannot end
    /// up marked as derived (and silently unpersisted).
    async fn ensure_spawn_extension(&self, session_id: &str) {
        let config = ExtensionConfig::Platform {
            name: Self::SPAWN_EXTENSION.to_string(),
            description: "Delegate work to subagents".to_string(),
            bundled: Some(true),
            // The spawn-only surface. Enforced on BOTH the advertisement path
            // (`filter`/`is_tool_available` in `fetch_all_tools`) and the
            // dispatch path (the same predicate in `dispatch_tool_call`).
            available_tools: vec![SUBAGENT_TOOL_NAME.to_string()],
        };
        // Never fatal: a session that cannot load the extension simply has no
        // spawn tool this turn, which is a strictly smaller failure than
        // refusing the turn.
        if let Err(e) = self
            .extension_manager
            .add_extension_auto_injected(config)
            .await
        {
            tracing::warn!(
                session_id,
                "could not inject the workspace extension for subagents: {e}"
            );
        }
    }

    /// The tool list as the MODEL will see it. Issue #56 Gate E applies: a
    /// public model gets no private extension's tool names, descriptions or JSON
    /// schemas. Every model-facing caller — the turn's tool list, a subagent's,
    /// the prompt builders — uses this one.
    pub async fn list_tools(&self, session_id: &str, extension_name: Option<String>) -> Vec<Tool> {
        self.list_tools_for(session_id, extension_name, ToolAudience::Model)
            .await
    }

    /// The tool list as the PERMISSION EDITOR will show it — Settings →
    /// Extensions → tool permissions, and `biorouter configure`'s tool selector.
    ///
    /// ⚠ Not tier-filtered, by decision. See
    /// [`ExtensionManager::get_prefixed_tools_unfiltered`] for why: the reader
    /// here is the human who installed the private extension, not the model, and
    /// a tool that is not listed cannot have its permission set.
    pub async fn list_tools_for_permission_settings(
        &self,
        session_id: &str,
        extension_name: Option<String>,
    ) -> Vec<Tool> {
        self.list_tools_for(session_id, extension_name, ToolAudience::PermissionEditor)
            .await
    }

    async fn list_tools_for(
        &self,
        session_id: &str,
        extension_name: Option<String>,
        audience: ToolAudience,
    ) -> Vec<Tool> {
        // BR-71 decision 21: the workspace extension is the ONE spawn
        // implementation, so a session that may delegate must have it LOADED
        // before the tool list is read. When the user enabled `workspace`
        // explicitly it is already present with the full surface and this is a
        // no-op; otherwise it is injected with `available_tools: ["subagent"]`,
        // so delegation rides along WITHOUT the cross-session control surface
        // (§5 blast radius unchanged).
        //
        // This MUST run before `get_prefixed_tools`: that call is the only read
        // of the extension manager in this function, and `ensure_spawn_extension`
        // only *loads* the extension — it does not re-run the read. Injecting
        // after it would produce a tool list with no spawn tool on that turn,
        // i.e. "the first turn of every session cannot delegate", and for a
        // one-shot `biorouter run` every turn is the first turn.
        //
        // The `else` is the same statement read backwards, and it is not
        // optional: the injection is derived state, so when its cause is gone
        // the grant must go with it. See `revoke_spawn_extension`.
        let subagents_enabled = self.subagents_enabled(session_id).await;
        if subagents_enabled {
            self.ensure_spawn_extension(session_id).await;
        } else {
            self.revoke_spawn_extension().await;
        }

        let mut prefixed_tools = match audience {
            ToolAudience::Model => {
                self.extension_manager
                    .get_prefixed_tools(extension_name.clone())
                    .await
            }
            ToolAudience::PermissionEditor => {
                self.extension_manager
                    .get_prefixed_tools_unfiltered(extension_name.clone())
                    .await
            }
        }
        .unwrap_or_default();

        // Revoking the injection is necessary but not sufficient. `subagent` is
        // one of the workspace extension's OWN tools now, so a user who enabled
        // Workspace Control explicitly gets the spawn tool advertised whatever
        // the gate says — and that entry is the user's, so it must not be
        // revoked. The invariant is about the tool, not about how the extension
        // arrived: no spawn tool, under any name, from any source, in a session
        // whose gate says delegation is off.
        if !subagents_enabled {
            let prefixed_spawn_tool = format!("{}__{}", Self::SPAWN_EXTENSION, SUBAGENT_TOOL_NAME);
            prefixed_tools.retain(|tool| tool.name != prefixed_spawn_tool);
        }

        // BR-71: the extension advertises the spawn tool with no sub-workflow
        // knowledge; only the Agent has the map. Restore the enriched
        // description here so a session that defines sub-workflows still tells
        // the model their names — the pre-merge behaviour.
        if subagents_enabled {
            let sub_workflows: Vec<_> = self.sub_workflows.lock().await.values().cloned().collect();
            if !sub_workflows.is_empty() {
                if let Some(spawn) = prefixed_tools
                    .iter_mut()
                    .find(|t| t.name == crate::agents::subagent_tool::SUBAGENT_TOOL_PREFIXED)
                {
                    spawn.description = Some(
                        crate::agents::subagent_tool::build_tool_description(&sub_workflows).into(),
                    );
                }
            }
        }

        if (extension_name.is_none() || extension_name.as_deref() == Some("platform"))
            && self.config.scheduler_service.is_some()
        {
            prefixed_tools.push(platform_tools::manage_schedule_tool());
        }

        // The conversation-ingestion tool is always available on the platform
        // extension: it needs only the session store (always present) and the
        // agent's provider, which is checked at call time.
        if extension_name.is_none() || extension_name.as_deref() == Some("platform") {
            prefixed_tools.push(platform_tools::ingest_conversation_tool());
        }

        // BR-7: the retrieval half of externalized tool results. Only offered
        // when lazy blob loading is on — with the default hydrating read the
        // payloads are spliced back into the conversation at load time, so the
        // model never sees a stub and would have nothing to read back.
        if (extension_name.is_none() || extension_name.as_deref() == Some("platform"))
            && message_blobs::lazy_load_enabled()
        {
            prefixed_tools.push(platform_tools::read_session_blob_tool());
        }

        if extension_name.is_none() {
            if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                prefixed_tools.push(final_output_tool.tool());
            }

            // BR-71 decision 20: the standalone bare-`subagent` push that used
            // to live here is GONE, and the Task 21 gate greps this file to
            // prove it. The workspace extension is now the one and only
            // advertisement of the spawn tool (Task 18 loads it above,
            // `get_prefixed_tools` lists it as `workspace__subagent`); pushing
            // a second bare copy here would advertise the same tool twice
            // under two names.
            //
            // BR-71 decision 23: the poll half of BR-40's spawn→poll model used
            // to be pushed here too, gated on `background_enabled()`. It is gone
            // with the tool — a background child is now waited on with
            // `workspace_watch`, read with `workspace_read_conversation` and
            // stopped with `workspace_close`, all of which the workspace
            // extension already advertises and all of which work for foreground
            // children and for the human as well.
        }

        prefixed_tools
    }

    pub async fn remove_extension(&self, name: &str) -> Result<()> {
        // Provenance goes with the entry, so removing it takes the
        // auto-injection mark with it — a removed-then-re-injected extension is
        // never left permanently exempt from persistence.
        self.extension_manager.remove_extension(name).await?;
        Ok(())
    }

    pub async fn list_extensions(&self) -> Vec<String> {
        self.extension_manager
            .list_extensions()
            .await
            .expect("Failed to list extensions")
    }

    pub async fn get_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extension_manager.get_extension_configs().await
    }

    /// Register a pending tool-permission prompt and get the receiver the loop
    /// parks on. Called *before* the confirmation message is yielded so a client
    /// that answers instantly still finds a live sender (BR-62).
    pub(super) fn register_confirmation(
        &self,
        request_id: &str,
    ) -> oneshot::Receiver<PermissionConfirmation> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending_confirmations.lock() {
            pending.insert(request_id.to_string(), tx);
        }
        rx
    }

    /// Drop a pending prompt without a decision (it expired, the turn was
    /// cancelled, or it was already answered). Idempotent.
    pub(super) fn forget_confirmation(&self, request_id: &str) {
        if let Ok(mut pending) = self.pending_confirmations.lock() {
            pending.remove(request_id);
        }
    }

    /// Whether a tool-permission prompt with this request id is still awaiting a
    /// decision. Lets a route answer a duplicate/late POST idempotently instead
    /// of pretending it resolved something.
    pub fn has_pending_confirmation(&self, request_id: &str) -> bool {
        self.pending_confirmations
            .lock()
            .map(|pending| pending.contains_key(request_id))
            .unwrap_or(false)
    }

    /// Handle a confirmation response for a tool request.
    ///
    /// BR-62: routed by request id to that prompt's own channel. A decision for
    /// an id nobody is waiting on (double-click, a prompt that already expired or
    /// was cancelled, a stale client replaying an old card) is **dropped** and
    /// reported as [`ConfirmationOutcome::Unknown`] — it must never be applied to
    /// whatever other tool call happens to be pending now.
    pub async fn handle_confirmation(
        &self,
        request_id: String,
        confirmation: PermissionConfirmation,
    ) -> ConfirmationOutcome {
        let sender = self
            .pending_confirmations
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request_id));

        match sender {
            Some(tx) => {
                if tx.send(confirmation).is_ok() {
                    ConfirmationOutcome::Delivered
                } else {
                    // The waiter went away between our lookup and the send (turn
                    // ended/cancelled). Nothing to do, and nothing to blame.
                    debug!(
                        "Confirmation for request {} arrived after the waiter went away",
                        request_id
                    );
                    ConfirmationOutcome::Unknown
                }
            }
            None => {
                debug!(
                    "Ignoring confirmation for request {}: no prompt is awaiting a decision",
                    request_id
                );
                ConfirmationOutcome::Unknown
            }
        }
    }

    #[instrument(skip(self, user_message, session_config), fields(user_message))]
    #[allow(clippy::too_many_lines)]
    pub async fn reply(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();
        // #59: everything this function persists BEFORE the reply stream is
        // constructed — the user's own message, a slash command's resolution,
        // the hook context. Each row adopts its effective uid as it is written
        // and is published as the stream's first event, so a client that never
        // minted an id for its prompt still learns the one the store did.
        let mut user_message = user_message;
        let mut prestream_persisted: Vec<Message> = Vec::new();

        // Taken by value first: the persist below needs `&mut user_message`, so
        // the scan cannot keep a borrow of its content alive across it.
        let elicitation_response = user_message
            .content
            .iter()
            .find_map(|content| match content {
                MessageContent::ActionRequired(action_required) => match &action_required.data {
                    ActionRequiredData::ElicitationResponse { id, user_data } => {
                        Some((id.clone(), user_data.clone()))
                    }
                    _ => None,
                },
                _ => None,
            });
        if let Some((id, user_data)) = elicitation_response {
            if let Err(e) = ActionRequiredManager::global()
                .submit_response(id.clone(), user_data)
                .await
            {
                // No live request is waiting on this id. The usual cause
                // is a daemon restart between the elicitation and the
                // reply: the in-memory pending request — and the tool
                // call parked on it — died with the old process, so the
                // answer has nowhere to go (BR-41). Surface that the run
                // was interrupted instead of a raw error, and still keep
                // the reply in history.
                tracing::warn!("Elicitation response for {id} could not be delivered: {e}");
                session_manager
                    .add_message_adopting_uid(&session_config.id, &mut user_message)
                    .await?;
                let notice = Message::assistant()
                    .with_system_notification(
                        SystemNotificationType::InlineMessage,
                        "The request that was waiting for your input was interrupted \
                                 (most likely by a restart), so your answer couldn't be \
                                 delivered. Please re-send your request to continue.",
                    )
                    .user_only();
                // #66 SHAPE 1 (yield, then name). Traced: the row named here is
                // the prompt, and the message yielded here is the notice — a
                // different row, which is never persisted — so nothing could
                // arrive ahead of its own body either way. Ordered regardless: a
                // rule carrying per-site exemptions ("this one's trailing frame
                // happens not to be persisted") is the reasoning that produced
                // the defect #66 closes.
                return Ok(Box::pin(stream::iter(
                    yielded_then_named([notice], std::slice::from_ref(&user_message))
                        .into_iter()
                        .map(Ok),
                )));
            }
            session_manager
                .add_message_adopting_uid(&session_config.id, &mut user_message)
                .await?;
            // #66 SHAPE 2: this batch yields nothing at all — the answer goes to
            // the parked tool call and the turn resumes there. The only row it
            // names is the client's own prompt.
            let published = named_but_never_yielded(
                std::slice::from_ref(&user_message),
                NeverYielded::ClientAuthoredPrompt,
            );
            return Ok(Box::pin(stream::iter(published.into_iter().map(Ok))));
        }

        // Issue #56 Gate B. Placed AFTER the elicitation early-returns above:
        // an elicitation answer is a user action on a parked tool call, not a
        // disclosure, and refusing it at the literal top of `reply` silently
        // drops the answer and the daemon-restart notice. Placed BEFORE
        // `restore_goal` so nothing runs on a session we are about to refuse.
        //
        // Repair-first, and the repair is the common case: LRU rehydration,
        // `restore_provider_from_session`'s Config::global() fallback, a
        // pre-fix diverge and every legacy row all land here.
        let privacy_row = session_manager
            .get_session(&session_config.id, false)
            .await
            .ok();
        let mut privacy_refusal: Option<String> = None;
        // DR-15's master opt-out. ONE read for this seam, used by both halves —
        // the turn barrier below and DR-4's turn ratchet under it — so the two
        // cannot be observed at different instants and a turn cannot be refused
        // while its classification write is skipped, or the reverse.
        //
        // A direct read, not a `CallCapability`: a turn is not a tool call and
        // has no admitted capability to inherit. AR-7 is explicit that the
        // toggle stops the classification ratchet along with the gates — a
        // classification written while the user believes the feature is off is
        // permanent, because `privacy_tier` is monotone and re-enabling never
        // revisits a row.
        let privacy_enforced = crate::privacy::privacy_tiers_enabled();
        if let Some(row) = privacy_row.as_ref() {
            let bound = self.bound_provider_unchecked().await;
            // No provider bound at all reads as Public — the fail-SAFE side.
            // Public is the less privileged tier, so an agent with nothing
            // bound gets the repair attempt rather than a free pass.
            let bound_tier = bound
                .as_ref()
                .map(|provider| provider.tier())
                .unwrap_or(ProviderTier::Public);
            if privacy_enforced && !crate::privacy::bind_allowed(bound_tier, row.privacy_tier) {
                match self.rebind_from_row(row).await {
                    // 2. The row still names a provider whose tier satisfies
                    //    the classification: rebind and continue silently.
                    Ok(true) => {}
                    // 3. Otherwise refuse THIS TURN. The row is untouched, so
                    //    the repair card can still offer the one-click fix.
                    _ => privacy_refusal = Some(crate::privacy::refusal::turn_refusal(row)),
                }
            }
            // 1. The ratchet. It fires HERE and on a permitted private-extension
            //    dispatch (Gate C) — never on the bind (O5).
            //
            // ⚠ It samples the binding ONCE, at the seam. A `/model` switch to a
            // private provider that lands mid-turn therefore runs that turn
            // against a row still classified public, and the ratchet catches it
            // on the NEXT turn. That is under-classification for one turn, never
            // over-disclosure: Gate A independently refuses the dangerous
            // direction (a public provider bound to a private row), so the only
            // thing the window costs is that Gate D would let a public model
            // read this turn's transcript before the ratchet lands. Inherent to
            // "ratchet on the turn" rather than on the bind, which is O5's
            // deliberate trade, and recorded here because it was not.
            let mut classification = row.privacy_tier;
            if privacy_enforced && privacy_refusal.is_none() {
                if let Some(provider) = self.bound_provider_unchecked().await {
                    let f = crate::privacy::floor(provider.tier());
                    if f > row.privacy_tier {
                        session_manager
                            .update(&session_config.id)
                            .raise_privacy(f, &format!("turn:{}", provider.get_name()))
                            .apply()
                            .await?;
                        classification = f;
                    }
                }
            }
            // ⚠ The POST-ratchet classification, not the row's. A turn that
            // has just raised the session to Private must not leave the cache
            // saying Public, or the auto-naming call that runs the moment this
            // stream ends is the first thing to walk through Gate B'.
            self.cached_classification
                .store(&session_config.id, classification);
            // Issue #56 Gate H. The hooks manager resolves a prompt hook's own
            // provider — an endpoint named by config.yaml or by an agent-writable
            // `.biorouter/hooks.yaml`, which the session row never records — and
            // the Stop hook it fires below carries `transcript_tail`. Mirrored
            // from the SAME value stored above, at the same seam, so the two
            // cannot disagree about this turn — and keyed by the SAME session
            // id, so an agent shared across chats (`biorouter web`) cannot have
            // one turn's classification answer another turn's hook.
            self.hooks_manager
                .set_session_classification(&session_config.id, classification);
        } else {
            // No row. There is no classification to honour and no content to
            // protect, but there is also no way to tell "this id names nothing"
            // from "the read failed" — so fail closed. It costs no liveness:
            // `RewriteBasis::read_with_session` below reads the same row with a
            // `?`, so a turn that gets here without one was already over.
            self.cached_classification
                .store(&session_config.id, SessionClassification::Private);
            self.hooks_manager
                .set_session_classification(&session_config.id, SessionClassification::Private);
        }
        // ⚠ A refusal is a YIELD, never an `Err` out of `reply`: `reply`
        // returns `Result<BoxStream<..>>`, and an `Err` here surfaces as a 500
        // from `/reply` instead of a message the user can act on. The precedent
        // is the compaction-failure arm further down, which yields its message
        // and returns rather than failing the stream.
        //
        // It returns its own stream rather than setting a flag the main
        // `try_stream!` reads, and that is a DEVIATION from the plan's sketch
        // with a reason: between this seam and that stream the prologue fires
        // SessionStart/UserPromptSubmit hooks, runs `execute_command`, persists
        // the prompt, and calls `self.provider().await?` for the compaction
        // check — which, with Gate B' live, is exactly the `Err` out of `reply`
        // the paragraph above forbids. "Nothing runs on a session we are about
        // to refuse" is only true if the refusal returns here.
        if let Some(text) = privacy_refusal {
            let refused: BoxStream<'_, Result<AgentEvent>> = Box::pin(async_stream::try_stream! {
                yield AgentEvent::Message(Message::assistant().with_text(text));
            });
            return Ok(refused);
        }

        // A daemon restart drops the in-memory goal registry and its Stop-hook
        // judge, while the goal itself persists in the session's extension_data
        // (like todos). Restore it before handling this turn so an active /goal
        // survives the restart. No-op when the goal is already live in this
        // process or none was stored (BR-41).
        self.restore_goal(&session_config.id).await;

        let message_text = user_message.as_concat_text();

        // User-configured hooks: SessionStart fires once per session, then
        // UserPromptSubmit may block the prompt or inject context. Slash
        // commands and elicitation responses don't count as prompts.
        let mut hook_context: Option<String> = None;
        if !message_text.trim().starts_with('/') {
            if let Ok(hook_session) = session_manager.get_session(&session_config.id, false).await {
                let hooks = self.hooks_manager();
                hooks.reset_stop_blocks(&session_config.id).await;
                // BR-19: a turn cancelled between the PreToolUse inspector and
                // the tool-path injection point can leave staged hook context
                // behind. Drop it on a fresh prompt rather than injecting a note
                // about a tool call the user aborted; the consecutive
                // PostToolUse-block count is per-turn for the same reason.
                hooks.clear_staged_tool_hooks(&session_config.id);
                hooks.reset_post_tool_blocks(&session_config.id).await;

                let source = if hook_session.message_count > 0 {
                    "resume"
                } else {
                    "startup"
                };
                let session_start_context = hooks
                    .session_start_once(&hook_session.id, &hook_session.working_dir, source)
                    .await
                    .and_then(|aggregate| aggregate.joined_context());

                let prompt_aggregate = hooks
                    .user_prompt_submit(&hook_session.id, &hook_session.working_dir, &message_text)
                    .await;

                if prompt_aggregate.is_denied() {
                    let reason = prompt_aggregate
                        .deny_reason()
                        .unwrap_or("blocked")
                        .to_string();
                    // #59: the stored row is the user-only copy; the id it takes
                    // is stamped back onto `user_message` so the copy yielded
                    // below names the row it became.
                    let mut stored = user_message.clone().with_visibility(true, false);
                    session_manager
                        .add_message_adopting_uid(&session_config.id, &mut stored)
                        .await?;
                    user_message.id = stored.id.clone();
                    let notice = Message::assistant()
                        .with_system_notification(
                            SystemNotificationType::InlineMessage,
                            format!("Prompt blocked by hook: {reason}"),
                        )
                        .user_only();
                    // #66 SHAPE 1 / #59 ORDERING: `user_message` carries
                    // `stored`'s id, so publishing first would hand the client
                    // that id before the message it names.
                    return Ok(Box::pin(stream::iter(
                        yielded_then_named([user_message, notice], std::slice::from_ref(&stored))
                            .into_iter()
                            .map(Ok),
                    )));
                }

                let mut contexts: Vec<String> = Vec::new();
                if let Some(ctx) = session_start_context {
                    contexts.push(ctx);
                }
                if let Some(ctx) = prompt_aggregate.joined_context() {
                    contexts.push(ctx);
                }
                if !contexts.is_empty() {
                    hook_context = Some(contexts.join("\n\n"));
                }
            }
        }

        // Track custom slash command usage (don't track command name for privacy)
        if message_text.trim().starts_with('/') {
            let command = message_text.split_whitespace().next();
            if let Some(cmd) = command {
                if crate::slash_commands::get_workflow_for_command(cmd).is_some() {
                    // (telemetry for custom slash command usage removed)
                }
            }
        }

        let command_result = self.execute_command(&message_text, &session_config).await;

        match command_result {
            Err(e) => {
                let error_message = Message::assistant()
                    .with_text(e.to_string())
                    .with_visibility(true, false);
                return Ok(Box::pin(stream::once(async move {
                    Ok(AgentEvent::Message(error_message))
                })));
            }
            Ok(Some(response)) if response.role == rmcp::model::Role::Assistant => {
                let mut stored_user = user_message.clone().with_visibility(true, false);
                session_manager
                    .add_message_adopting_uid(&session_config.id, &mut stored_user)
                    .await?;
                let mut stored_response = response.clone().with_visibility(true, false);
                session_manager
                    .add_message_adopting_uid(&session_config.id, &mut stored_response)
                    .await?;
                // #59: the yielded copies name the rows they became. Only the
                // store's own uid is ever stamped here — minting a *different*
                // fallback id would hand the client a name no row answers to,
                // which is worse than the `None` it replaced.
                let mut response = response;
                user_message.id = stored_user.id.clone();
                response.id = stored_response.id.clone();
                // #66 SHAPE 1 / #59 ORDERING: both messages carry the ids named
                // here, so they go over the wire first — a client that is cut
                // off after the accounting frame must never be left claiming
                // rows whose bodies it never received. Ordered out here rather
                // than inside the stream block below because the rows being
                // named are the stored copies, which the block does not own.
                let ordered =
                    yielded_then_named([user_message, response], [&stored_user, &stored_response]);

                // Check if this was a command that modifies conversation history
                let modifies_history = crate::agents::execute_commands::COMPACT_TRIGGERS
                    .contains(&message_text.trim())
                    || message_text.trim() == "/clear";

                return Ok(Box::pin(async_stream::try_stream! {
                    for event in ordered {
                        yield event;
                    }

                    // After commands that modify history, notify UI that history was replaced
                    if modifies_history {
                        let updated_session = session_manager.get_session(&session_config.id, true)
                            .await
                            .map_err(|e| anyhow!("Failed to fetch updated session: {}", e))?;
                        let updated_conversation = updated_session
                            .conversation
                            .ok_or_else(|| anyhow!("Session has no conversation after history modification"))?;
                        yield AgentEvent::HistoryReplaced(updated_conversation);
                    }
                }));
            }
            Ok(Some(resolved_message)) => {
                let mut stored_user = user_message.clone().with_visibility(true, false);
                session_manager
                    .add_message_adopting_uid(&session_config.id, &mut stored_user)
                    .await?;
                user_message.id = stored_user.id.clone();
                let mut resolved = resolved_message.with_visibility(false, true);
                session_manager
                    .add_message_adopting_uid(&session_config.id, &mut resolved)
                    .await?;
                prestream_persisted.push(stored_user);
                prestream_persisted.push(resolved);
            }
            Ok(None) => {
                session_manager
                    .add_message_adopting_uid(&session_config.id, &mut user_message)
                    .await?;
                prestream_persisted.push(user_message.clone());
            }
        }

        // Context injected by SessionStart/UserPromptSubmit hooks: visible to
        // the model, hidden from the user.
        if let Some(context) = hook_context {
            let mut injected = Message::user()
                .with_text(crate::hooks::outcome::frame_hook_context(&context))
                .with_visibility(false, true);
            session_manager
                .add_message_adopting_uid(&session_config.id, &mut injected)
                .await?;
            prestream_persisted.push(injected);
        }
        // #59: published as the stream's first event, before anything else can
        // be appended to the session, so the client's view of the stored set
        // starts complete.
        //
        // #66 SHAPE 2. Being first is not a breach of the ordering rule stated
        // on [`persisted_ordering`]: NONE of these rows is ever yielded as a
        // `Message`. The user's own prompt is content the client authored and
        // already holds, and the slash-command resolution and hook context are
        // model-only. So no id here can arrive ahead of its message — there is
        // no message frame for it to arrive ahead of.
        let prestream_published = named_but_never_yielded(
            prestream_persisted.iter(),
            NeverYielded::ClientPromptAndModelOnly,
        );

        // Snapshot with the revision this view is based on, so every rewrite in
        // this turn — the auto-compaction below AND the reply loop's overflow
        // recoveries — can tell a concurrent append apart from its own edits.
        // The basis travels WITH the history it describes, all the way down: a
        // revision re-read later, against a view decided earlier, is the split
        // that lets a concurrent append fall between the two (see
        // [`RewriteBasis`]).
        let (session, mut rewrite_basis) =
            RewriteBasis::read_with_session(&session_manager, &session_config.id).await?;
        let stored_conversation = rewrite_basis.known().clone();
        let conversation = crate::conversation::without_bedrock_reasoning(rewrite_basis.known());

        // BR-12: this synchronous check is the *fallback*. In the common case
        // the previous turn's `maybe_spawn_eager_compaction` already compacted in
        // the background between turns, so `needs_auto_compact` is false here and
        // the turn starts immediately. It still fires — and blocks the turn — when
        // eager compaction hasn't landed (a huge single turn, a fast follow-up
        // message before the background task finished, a disabled eager path, or a
        // failed task), so a session can never overflow.
        //
        // BR-15: on the cold path (a session's first turn, or a provider that
        // doesn't report usage) the token estimate needs the system prompt and
        // tool schemas or it undercounts badly. Assemble them only when needed —
        // the happy path reads session.total_tokens and shouldn't pay for
        // tool/prompt assembly here (the reply loop re-does it anyway).
        let cold_path_tools_and_prompt = if session.total_tokens.is_none() {
            Some(
                self.prepare_tools_and_prompt(&session_config.id, &session.working_dir)
                    .await?,
            )
        } else {
            None
        };

        let needs_auto_compact = check_if_compaction_needed(
            self.provider().await?.as_ref(),
            &conversation,
            None,
            &session,
            cold_path_tools_and_prompt
                .as_ref()
                .map(|(tools, _toolshim, system_prompt)| {
                    (system_prompt.as_str(), tools.as_slice())
                }),
        )
        .await?;

        let conversation_to_compact = stored_conversation.clone();

        Ok(Box::pin(async_stream::try_stream! {
            if let Some(published) = prestream_published {
                yield published;
            }
            let final_conversation = if !needs_auto_compact {
                stored_conversation
            } else {
                let config = Config::global();
                let threshold = config
                    .get_param::<f64>("BIOROUTER_AUTO_COMPACT_THRESHOLD")
                    .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
                let threshold_percentage = (threshold * 100.0) as u32;

                let inline_msg = format!(
                    "Exceeded auto-compact threshold of {}%. Performing auto-compaction...",
                    threshold_percentage
                );

                yield AgentEvent::Message(
                    inline_notice(inline_msg,)
                );

                yield AgentEvent::Message(
                    thinking_notice(COMPACTION_THINKING_TEXT,)
                );

                self.fire_compaction_hook(
                    crate::hooks::HookEvent::PreCompact,
                    &session_config.id,
                    &session.working_dir,
                    "auto",
                    None,
                );
                let usage_event_key = uuid::Uuid::new_v4().to_string();
                match compact_messages(self.provider().await?.as_ref(), &conversation_to_compact, false).await {
                    Ok((compacted_conversation, summarization_usage)) => {
                        // The swap and the basis re-pairing are one step, and both
                        // happen before anything is yielded; see
                        // `land_auto_compaction` for why.
                        let (outcome, latest_conversation) = self.land_auto_compaction(
                            &session_config.id,
                            &conversation_to_compact,
                            &compacted_conversation,
                            &mut rewrite_basis,
                        ).await?;

                        if !outcome.stored() {
                            // The basis was truncated or rewritten under us (a
                            // checkpoint restore, a message edit, another turn).
                            // Declining here must NOT fail the turn — that would
                            // trade a data-loss bug for a liveness bug. Continue on
                            // the fresh history and let the overflow-recovery ladder
                            // handle it if it really is too large.
                            warn!(
                                "Auto-compaction skipped for session {} ({:?}); the history \
                                 changed while it was being summarized",
                                session_config.id, outcome
                            );
                            // The round-trip was spent and billed by the provider
                            // either way. `is_compaction_usage = false` because this
                            // usage did NOT replace the context: claiming it did
                            // would reset the live gauge to the summary's size and
                            // suppress the next compaction check on a history that
                            // never shrank.
                            self.update_session_metrics(
                                &session_config,
                                &summarization_usage,
                                false,
                                &usage_event_key,
                            ).await?;
                            // No PostCompact: nothing was compacted.
                            yield AgentEvent::Message(
                                inline_notice(
                                    "Compaction skipped: this chat changed while it was \
                                    being summarized. Continuing with the full history.",
                                )
                            );
                            // Continue on the fresh history the re-pair above read
                            // (or, if that failed, the view we started from).
                            latest_conversation
                        } else {
                        self.update_session_metrics(
                            &session_config,
                            &summarization_usage,
                            true,
                            &usage_event_key,
                        ).await?;
                        self.fire_compaction_hook(
                            crate::hooks::HookEvent::PostCompact,
                            &session_config.id,
                            &session.working_dir,
                            "auto",
                            None,
                        );

                        // BR-52: compaction rewrote the live gauge (the summary
                        // becomes the new input context) and billed its own turn.
                        if let Some(token_state) = self.current_token_state(&session_config.id).await {
                            yield AgentEvent::TokenUsage(token_state);
                        }

                        yield AgentEvent::HistoryReplaced(latest_conversation.clone());

                        yield AgentEvent::Message(
                            inline_notice("Compaction complete",)
                        );

                        latest_conversation
                        }
                    }
                    Err(e) => {
                        yield AgentEvent::Message(
                            assistant_text(format!("Ran into this error trying to compact: {e}.\n\nPlease try again or create a new chat"))
                        );
                        return;
                    }
                }
            };

            let mut reply_stream = self.reply_internal(final_conversation, rewrite_basis, session_config, session, cancel_token).await?;
            while let Some(event) = reply_stream.next().await {
                yield event?;
            }
        }))
    }

    /// Land one auto-compaction: swap the summarized history in under the store's
    /// freshness guard, then re-pair the rewrite basis with the history it
    /// describes.
    ///
    /// The swap runs under the guard so a message another writer appended while
    /// the summarizer ran is carried over rather than deleted, and the
    /// conversation that actually landed is what the turn (and the client)
    /// continues from. The basis is re-paired the moment the rewrite returns,
    /// before anything is yielded: a landed rewrite renumbered every row, and a
    /// declined one means the store moved under us — either way the reply loop
    /// must inherit a revision and a history read TOGETHER, not a revision read
    /// after the view it is meant to describe.
    ///
    /// Split out of the `reply` generator to keep its `poll` frame small — see
    /// [`ToolBatchMaps`].
    async fn land_auto_compaction(
        &self,
        session_id: &str,
        conversation_to_compact: &Conversation,
        compacted_conversation: &Conversation,
        rewrite_basis: &mut RewriteBasis,
    ) -> Result<(
        crate::session::session_manager::ReplaceOutcome,
        Conversation,
    )> {
        let (outcome, stored_conversation) = self
            .config
            .session_manager
            .replace_conversation_preserving_tail(
                session_id,
                compacted_conversation,
                rewrite_basis.revision,
                rewrite_basis.known(),
            )
            .await?;
        let latest_conversation = self
            .reseed_basis(
                session_id,
                rewrite_basis,
                if outcome.stored() {
                    stored_conversation
                } else {
                    // Nothing was written, so the rewrite handed back its own
                    // `replacement`. The pre-compaction view is the honest fallback
                    // if the re-read fails.
                    conversation_to_compact.clone()
                },
            )
            .await;
        Ok((outcome, latest_conversation))
    }

    /// BR-31 + BR-66: the soft-stage nudges a finished tool batch earns.
    ///
    /// BR-31 speaks when *one* tool has failed *the same way* N times in a row —
    /// nudging the model here, with the failing result still in front of it,
    /// rather than waiting for it to burn another call; the hard stop for a
    /// streak that survives the nudges is enforced by the repetition inspector on
    /// the next call. BR-66 covers the shape BR-31 cannot see: a run of
    /// *different* tools failing in *different* ways, the ordinary shape of an
    /// agent that has lost the thread. Every failed call of any kind (malformed
    /// calls included) is counted and, at the cap, the model is made to stop and
    /// re-plan. Warn-only: a mixed run of failures is not proof the next call is
    /// doomed, so nothing is blocked.
    ///
    /// Split out of the `reply_internal` generator to keep its `poll` frame small
    /// — see [`ToolBatchMaps`].
    #[allow(clippy::too_many_arguments)]
    async fn extend_batch_nudges(
        &self,
        session_id: &str,
        conversation: &Conversation,
        remaining_requests: &[ToolRequest],
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &HashMap<String, Arc<Mutex<Message>>>,
        mistake_config: &crate::agents::mistakes::MistakeConfig,
        mistakes: &mut crate::agents::mistakes::MistakeTracker,
        loop_warnings: &mut Vec<String>,
    ) {
        loop_warnings.extend(
            self.failure_loop_nudges(
                conversation.messages(),
                remaining_requests,
                request_to_response_map,
            )
            .await,
        );
        let outcomes = self
            .mistake_outcomes(
                remaining_requests,
                permission_check_result,
                request_to_response_map,
            )
            .await;
        if let Some(nudge) = mistakes.observe_tool_outcomes(mistake_config, &outcomes) {
            tracing::info!(
                streak = mistakes.streak(),
                "Injecting mistake-streak reflect-and-replan nudge"
            );
            emit_loop_safety(
                LoopSafetyKind::MistakeStreakNudge,
                session_id,
                mistakes.streak(),
                None,
                None,
            );
            loop_warnings.push(nudge);
        }
    }

    /// The model-visible instruction and the user-visible notice a blocked Stop
    /// hook produces.
    ///
    /// For goal loops the block is accounted against the goal's own
    /// iteration/stall budget (which, unlike the generic Stop-hook cap, does not
    /// reset when tools run). On give-up the goal is cleared and the agent
    /// delivers a best-effort answer instead of looping forever.
    ///
    /// Split out of the `reply_internal` generator to keep its `poll` frame small
    /// — see [`ToolBatchMaps`].
    async fn stop_hook_block_feedback(
        &self,
        session_id: &str,
        reason: &str,
        has_active_goal: bool,
    ) -> (String, String) {
        let goal_outcome = if has_active_goal {
            self.record_goal_block(session_id, reason).await
        } else {
            None
        };
        match goal_outcome {
            Some(crate::agents::goal::GoalOutcome::GiveUp { attempts, stalled }) => {
                self.clear_goal(session_id).await;
                let why = if stalled {
                    "it stopped making progress"
                } else {
                    "it hit the attempt limit"
                };
                (
                    crate::agents::goal::giveup_instruction(reason),
                    format!(
                        "🎯 Goal stopped after {attempts} attempt(s): {why}. \
                         Wrapping up with a best-effort answer; refine with a \
                         narrower /goal if needed."
                    ),
                )
            }
            _ => (
                format!("Stop hook feedback: {reason}"),
                format!("Stop hook blocked completion: {reason}"),
            ),
        }
    }

    /// Bill one overflow-recovery compaction's provider round-trips to both the
    /// reply budget and the session gauge.
    ///
    /// BR-35: a summarization round-trip inside the reply is spend like any
    /// other, so it is billed to the budget too, not just the session gauge —
    /// and that holds for a round-trip whose result was discarded, because the
    /// provider charged for it. Only the attempt that actually landed replaced
    /// the context; marking a discarded one as a compaction would reset the live
    /// gauge to the summary's size over a history that never shrank.
    ///
    /// Split out of the `reply_internal` generator to keep its `poll` frame small
    /// — see [`ToolBatchMaps`].
    async fn bill_overflow_compaction(
        &self,
        session_config: &SessionConfig,
        swap: &OverflowCompactionSwap,
        budget: &mut BudgetTracker,
    ) -> Result<()> {
        let persisted = swap.stored.is_some();
        let last = swap.usages.len().saturating_sub(1);
        for (i, usage) in swap.usages.iter().enumerate() {
            self.record_budget_usage(budget, usage).await;
            self.update_session_metrics(
                session_config,
                usage,
                persisted && i == last,
                &uuid::Uuid::new_v4().to_string(),
            )
            .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn reply_internal(
        &self,
        conversation: Conversation,
        rewrite_basis: RewriteBasis,
        session_config: SessionConfig,
        session: Session,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();
        let provider_conversation = crate::conversation::without_bedrock_reasoning(&conversation);
        let context = self
            .prepare_reply_context(
                &session_config.id,
                provider_conversation,
                &session.working_dir,
            )
            .await?;
        let ReplyContext {
            mut conversation,
            mut tools,
            mut toolshim_tools,
            mut system_prompt,
            biorouter_mode,
            initial_messages,
        } = context;
        let reply_span = tracing::Span::current();
        self.reset_retry_attempts().await;

        // Freshness basis for this turn's overflow-recovery compactions.
        //
        // Handed DOWN from `reply()`, already re-paired past its auto-compaction
        // if that fired (which renumbers every row, so a basis taken before it
        // would fail the store's prefix check and silently disable durable
        // recovery compaction on exactly the largest sessions). It is never
        // re-read here as a bare revision: the history it describes has to come
        // with it, or an append landing between the two reads is counted by the
        // watermark and missing from the view — the one shape the store's guard
        // cannot recover. See [`RewriteBasis`].
        //
        // It deliberately is NOT compared against the in-memory conversation's
        // length: the normalizer merges and drops messages at turn start, and
        // the retry manager pushes messages that are never persisted, so the two
        // are routinely unequal with zero concurrency.
        let mut rewrite_basis = rewrite_basis;

        // BR-63: the turn's reasoning effort. `Normal` (the default) changes
        // nothing: same provider object, same caps.
        let effort = self.resolve_effort(&session_config).await;
        let turn_provider = self.provider_with_effort(effort).await?;

        let working_dir = session.working_dir.clone();
        // BR-43: stable anchor for this turn's checkpoints — the `created`
        // timestamp of the last user message (the same key `truncate_conversation`
        // uses on restore). Computed once, before the loop mutates `conversation`.
        let checkpoint_anchor_ts = conversation
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == rmcp::model::Role::User)
            .map(|m| m.created)
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        Ok(Box::pin(async_stream::try_stream! {
            let _ = reply_span.enter();
            // Pre-turn snapshot: the clean work-tree state as this turn opens, so a
            // rewind to this turn can undo everything the agent does below.
            self.maybe_checkpoint(
                &session_config.id,
                &working_dir,
                checkpoint_anchor_ts,
                CheckpointKind::PreStep,
            ).await;
            // Enforcement state is scoped to this user turn. The repetition
            // inspector owns policy; this guard only applies its exact-signature
            // deny decision.
            let mut turn_guard = super::turn_guard::TurnToolGuard::new();
            let mut turns_taken = 0u32;
            // Cumulative tool calls dispatched this reply, across all iterations,
            // bounded by `max_tool_calls` so parallel fan-out can't run unbounded
            // even while `turns_taken` stays under `max_turns`.
            let mut tool_calls_taken = 0u32;
            let mut compaction_attempts = 0;
            let mut truncation_recovery = TruncationRecoveryBudget::default();
            let mut ever_made_user_visible_progress = false;
            // Everything this reply reads from config, resolved once — see
            // `ReplyLoopPolicy` for what each field is and why the resolution is
            // not written out here.
            let ReplyLoopPolicy {
                max_turns,
                max_tool_calls,
                tool_output_guardrail,
                tool_error_taxonomy,
                post_edit_diag_config,
                self_critique_config,
                done_gate_config,
                stall_config,
                mistake_config,
                mut budget,
            } = resolve_reply_loop_policy(effort, &session_config);
            // The tree-sitter analyzer BR-47 runs its check with is built lazily,
            // only if a `text_editor` write actually lands while the feature is
            // active.
            let mut post_edit_analyzer: Option<
                biorouter_mcp::developer::analyze::CodeAnalyzer,
            > = None;
            // BR-47: consecutive post-edit reflections this reply. Bounded by
            // `post_edit_diag_config.max_reflections` so a file that never parses
            // clean cannot wedge the turn — mirrors the PostToolUse block cap. Reset
            // to 0 whenever an edited file comes back clean, so a genuine fix
            // restores the budget.
            let mut post_edit_reflections: u32 = 0;
            // BR-50: corrective passes the critique has requested this reply,
            // bounded by `self_critique_config.max_passes` so a stubborn answer
            // cannot spin. Reply-scoped, like `post_edit_reflections`.
            let mut self_critique_passes: u32 = 0;
            // BR-48: the per-reply corrective-attempt counter — it does NOT reset
            // on tool calls (mirroring the /goal iteration budget), so a check that
            // never goes green cannot loop past the cap.
            let mut done_gate_iterations: u32 = 0;
            let mut stall_watch = crate::agents::stall::StallWatch::default();
            let mut mistakes = crate::agents::mistakes::MistakeTracker::default();
            // Set once the stall check has told the model to wrap up: the action
            // count by which this turn must be over, so a model that ignores the
            // give-up instruction and keeps calling tools cannot spin all the way
            // to `max_turns`.
            let mut stall_deadline: Option<u32> = None;
            let reply_started = std::time::Instant::now();
            // Set once the budget is spent and the model has been told to wrap up:
            // the action count by which this reply must be over, mirroring
            // `stall_deadline`, so a model that keeps calling tools anyway cannot
            // spend the budget twice over.
            let mut budget_deadline: Option<u32> = None;
            // A signed Bedrock assistant construction may span several provider
            // calls while tools run or a length-truncated answer continues. Pin
            // one provider instance and one exact provider-visible context for
            // that reply. A later `reply()` starts with an ordinary user block,
            // omits historical reasoning from its provider projection, and
            // rebuilds fresh MOIM/resource context.
            let reply_provider = match &turn_provider {
                Some(provider) => Arc::clone(provider),
                None => self.provider().await?,
            };
            let mut signed_replay_context: Option<Conversation> = None;

            // #69: this run of the loop is now the turn that soft interrupts are
            // accepted *into*. Opening here — where the loop actually starts, not
            // where the stream was built — means acceptance begins and ends with a
            // real consumer, and anything a previous turn left behind is dropped
            // with a warning instead of ambushing this one.
            let this_turn = TurnId::mint();
            self.open_for_turn(this_turn.clone());

            loop {
                if is_token_cancelled(&cancel_token) {
                    // BR-67: a cancelled turn and a completed turn look identical
                    // in the logs otherwise.
                    emit_loop_safety(
                        LoopSafetyKind::Cancelled,
                        &session_config.id,
                        turns_taken,
                        None,
                        None,
                    );
                    break;
                }

                if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                    if final_output_tool.final_output.is_some() {
                        let final_event = AgentEvent::Message(
                            assistant_text(final_output_tool.final_output.clone().unwrap())
                        );
                        yield final_event;
                        break;
                    }
                }

                turns_taken += 1;
                // Surface turn progress so an observer (CLI/GUI/logs) can tell how
                // much of the per-turn action budget has been used, and so a
                // budget-exhaustion stop is distinguishable from a normal completion.
                tracing::debug!("agent action {}/{} this turn", turns_taken, max_turns);
                if turns_taken > max_turns {
                    emit_loop_safety(
                        LoopSafetyKind::TurnLimitStop,
                        &session_config.id,
                        turns_taken,
                        Some(max_turns),
                        None,
                    );
                    yield AgentEvent::Message(
                        assistant_text(
                            format!(
                                "I've reached my action limit for this turn ({max_turns} actions without user input), so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with `max_turns` / `BIOROUTER_MAX_TURNS`.)"
                            )
                        )
                    );
                    break;
                }
                if tool_calls_taken > max_tool_calls {
                    emit_loop_safety(
                        LoopSafetyKind::ToolCallLimitStop,
                        &session_config.id,
                        tool_calls_taken,
                        Some(max_tool_calls),
                        None,
                    );
                    yield AgentEvent::Message(
                        assistant_text(
                            format!(
                                "I've made {tool_calls_taken} tool calls this turn, past my per-turn limit of {max_tool_calls}, so I'm stopping here rather than because the task is necessarily complete. Would you like me to continue? (raise the cap with `max_tool_calls` / `BIOROUTER_MAX_TOOL_CALLS`.)"
                            )
                        )
                    );
                    break;
                }
                // BR-32: the stall check already told the model to wrap up and it
                // kept going. End the turn rather than let a confirmed loop run to
                // the `max_turns` cap.
                if stall_deadline.is_some_and(|deadline| turns_taken > deadline) {
                    let reason = stall_watch
                        .last_reason()
                        .unwrap_or("repeating the same actions without progress")
                        .to_string();
                    warn!("stall give-up ignored; ending the turn at action {turns_taken}");
                    // BR-67: the judge's `reason` is model prose about the user's
                    // work — the event carries the action count only.
                    emit_loop_safety(
                        LoopSafetyKind::StallStop,
                        &session_config.id,
                        turns_taken,
                        None,
                        None,
                    );
                    yield AgentEvent::Message(
                        assistant_text(crate::agents::stall::stopped_message(&reason))
                    );
                    break;
                }
                // BR-35: the budget was spent, the model was asked to wrap up, and
                // it kept working past its grace window. End the reply rather than
                // let it spend the budget over again.
                if budget_deadline.is_some_and(|deadline| turns_taken > deadline) {
                    let snapshot = budget.snapshot_at(reply_started.elapsed());
                    warn!(
                        elapsed_seconds = snapshot.elapsed_seconds,
                        tokens = snapshot.tokens,
                        "reply budget wrap-up ignored; ending the turn at action {turns_taken}"
                    );
                    emit_loop_safety(
                        LoopSafetyKind::BudgetStop,
                        &session_config.id,
                        turns_taken,
                        None,
                        snapshot.axis,
                    );
                    yield AgentEvent::Message(
                        assistant_text(crate::agents::budget::stopped_message(&snapshot))
                    );
                    break;
                }

                // Soft interrupt: inject any user messages queued mid-turn at this
                // safe boundary (after the previous turn's tools completed, before
                // the next provider call) so the model incorporates them without a
                // cancel-and-resend round trip that discards in-flight work.
                for queued in self.drain_soft_interrupts() {
                    // The framing decision (and why it is not applied to the
                    // human's own steer) lives on `soft_interrupt_message`.
                    let mut m = soft_interrupt_message(queued);
                    // #41: adopt the minted uid — the retained/yielded copy
                    // must carry the same id as the stored row, or its next
                    // persist duplicates it instead of replaying.
                    session_manager
                        .add_message_adopting_uid(&session_config.id, &mut m)
                        .await?;
                    if signed_replay_context.take().is_some() {
                        conversation =
                            crate::conversation::without_bedrock_reasoning(&conversation);
                    }
                    conversation.push(m.clone());
                    yield AgentEvent::Message(m);
                }

                // BR-32: periodic "are you looping?" check for a turn that has been
                // running a long time without returning to the user. Off in normal
                // chat (nothing gets 30 actions deep); the LLM call is fail-open, so
                // an error here can never break a turn.
                match self.stall_check(
                    &session_config.id,
                    &conversation,
                    turns_taken,
                    &stall_config,
                    &mut stall_watch,
                ).await {
                    StallAction::Proceed => {}
                    StallAction::Nudge { reason } => {
                        info!(actions = turns_taken, "stall check flagged a loop; nudging the model");
                        emit_loop_safety(
                            LoopSafetyKind::StallNudge,
                            &session_config.id,
                            turns_taken,
                            None,
                            None,
                        );
                        // #59 / #66 SHAPE 2: a row the user is deliberately not
                        // shown, named so a client can still account for it.
                        let (nudge, published) = persist_steering_message(
                            &session_manager,
                            &session_config.id,
                            crate::agents::stall::nudge_instruction(&reason, turns_taken),
                        ).await?;
                        if let Some(published) = published {
                            yield published;
                        }
                        if signed_replay_context.take().is_some() {
                            conversation =
                                crate::conversation::without_bedrock_reasoning(&conversation);
                        }
                        conversation.push(nudge);
                        yield AgentEvent::Message(
                            inline_notice_user_only(
                                format!(
                                    "⏳ Progress check: {}",
                                    crate::agents::stall::ellipsize(&reason, 200)
                                ),
                            ),
                        );
                    }
                    StallAction::GiveUp { reason, flags, stalled } => {
                        warn!(
                            actions = turns_taken,
                            flags,
                            stalled,
                            "stall check gave up; asking for a best-effort answer"
                        );
                        // `flags` (how many progress checks flagged this turn) is
                        // the count that tripped the give-up; the reason prose
                        // stays out of the trace.
                        emit_loop_safety(
                            LoopSafetyKind::StallGiveUp,
                            &session_config.id,
                            flags,
                            Some(stall_config.max_flags),
                            None,
                        );
                        // The model gets a short grace window to write its wrap-up;
                        // after that the turn ends whether or not it complied.
                        stall_deadline =
                            Some(turns_taken + crate::agents::stall::STALL_WRAPUP_GRACE);
                        // #66 SHAPE 2: model-only, like the nudge above.
                        let (wrapup, published) = persist_steering_message(
                            &session_manager,
                            &session_config.id,
                            crate::agents::stall::giveup_instruction(&reason),
                        ).await?;
                        if let Some(published) = published {
                            yield published;
                        }
                        if signed_replay_context.take().is_some() {
                            conversation =
                                crate::conversation::without_bedrock_reasoning(&conversation);
                        }
                        conversation.push(wrapup);
                        let why = if stalled {
                            "the same loop kept repeating"
                        } else {
                            "no progress across several checks"
                        };
                        yield AgentEvent::Message(
                            inline_notice_user_only(
                                format!(
                                    "⏳ Stopped looping after {flags} progress check(s): {why}. \
                                     Wrapping up with a best-effort answer."
                                ),
                            ),
                        );
                    }
                }

                // BR-35: the per-reply budget meter. Cheap (no LLM call, no I/O):
                // a comparison against the running totals `record_turn_usage` has
                // already folded in, skipped entirely when no limit is set.
                match budget.check_at(reply_started.elapsed()) {
                    BudgetAction::Proceed => {}
                    BudgetAction::Warn(snapshot) => {
                        // One heads-up as the reply nears its ceiling, so a long
                        // agentic turn is never a silent spend.
                        info!(
                            elapsed_seconds = snapshot.elapsed_seconds,
                            tokens = snapshot.tokens,
                            axis = snapshot.axis,
                            "reply budget is running low"
                        );
                        emit_loop_safety(
                            LoopSafetyKind::BudgetWarn,
                            &session_config.id,
                            turns_taken,
                            None,
                            snapshot.axis,
                        );
                        yield AgentEvent::Message(
                            inline_notice_user_only(crate::agents::budget::progress_note(&snapshot),),
                        );
                    }
                    BudgetAction::Exceeded(snapshot) => {
                        warn!(
                            elapsed_seconds = snapshot.elapsed_seconds,
                            tokens = snapshot.tokens,
                            axis = snapshot.axis,
                            "reply budget spent; asking for a wrap-up"
                        );
                        emit_loop_safety(
                            LoopSafetyKind::BudgetExceeded,
                            &session_config.id,
                            turns_taken,
                            None,
                            snapshot.axis,
                        );
                        // Graceful: the model is told the budget is spent (and how
                        // many tokens it has left) and gets a short grace window to
                        // summarize where it got to. The hard stop above fires only
                        // if it ignores that.
                        budget_deadline = Some(
                            turns_taken + crate::agents::budget::BUDGET_WRAPUP_GRACE,
                        );
                        // #66 SHAPE 2: model-only, like the stall wrap-up above.
                        let (wrapup, published) = persist_steering_message(
                            &session_manager,
                            &session_config.id,
                            crate::agents::budget::wrapup_instruction(&snapshot),
                        ).await?;
                        if let Some(published) = published {
                            yield published;
                        }
                        if signed_replay_context.take().is_some() {
                            conversation =
                                crate::conversation::without_bedrock_reasoning(&conversation);
                        }
                        conversation.push(wrapup);
                        yield AgentEvent::Message(
                            inline_notice_user_only(
                                format!(
                                    "⏳ Budget reached ({}). Wrapping up with what I have.",
                                    snapshot.describe()
                                ),
                            ),
                        );
                    }
                }

                let conversation_with_moim = match signed_replay_context.as_ref() {
                    Some(replay) => replay.clone(),
                    None => {
                        self.assemble_turn_context(
                            &session_config.id,
                            &conversation,
                            &working_dir,
                        )
                        .await
                    }
                };

                let iteration_provider = Arc::clone(&reply_provider);
                let usage_event_key = uuid::Uuid::new_v4().to_string();
                // A coding-agent provider drives a child process that has to call
                // back in to use Biorouter's tools. The lease lives exactly as long
                // as the provider call: it is bound here and dropped at the end of
                // this iteration, and dropping it revokes the grant.
                //
                // The URL travels in a task-local because the `Provider` trait has
                // no session in scope — `complete_with_model` receives a system
                // prompt, messages and tools and nothing else. This works because
                // those providers are non-streaming, so the whole child turn happens
                // inside the awaited call and therefore inside this scope.
                let bridge_lease = self
                    .issue_tool_bridge(&session, &conversation_with_moim, &tools)
                    .await;
                let bridge_url = bridge_lease.as_ref().map(|l| l.url().to_string());
                let mut stream = coding_agent_bridge::ACTIVE_BRIDGE_URL
                    .scope(
                        bridge_url,
                        Self::stream_response_from_provider(
                            iteration_provider,
                            &system_prompt,
                            conversation_with_moim.messages(),
                            &tools,
                            &toolshim_tools,
                        ),
                    )
                    .await?;

                let mut no_tools_called = true;
                let mut messages_to_add = Conversation::default();
                let mut tools_updated = false;
                let mut signed_replay_invalidated_this_iteration = false;
                let mut did_recovery_compact_this_iteration = false;
                let mut did_retry_reset_this_iteration = false;
                // BR-66: set when a recoverable provider error was absorbed and a
                // hint pushed into `messages_to_add`; the turn continues instead of
                // ending on the error.
                let mut did_recover_provider_error_this_iteration = false;
                // finish_reason of this turn's response (from the provider usage),
                // used below to auto-continue a length-truncated turn.
                let mut last_finish_reason: Option<String> = None;
                // Visible partial output is genuine progress for the tighter
                // zero-progress budget. Signed thinking alone is deliberately
                // not: it is needed for model continuity, but gives the user no
                // partial answer to read.
                let mut made_user_visible_progress = false;
                // The turn's usage, recorded ONCE when the stream ends.
                //
                // It used to be written on every usage-bearing chunk, which (a) lost
                // the whole turn when the user cancelled — the terminal chunk that
                // carries usage never arrives — and (b) would multiply the count
                // against any OpenAI-compatible host that emits `usage` on more than
                // one chunk. Last snapshot wins; a cancelled turn keeps whatever the
                // provider had reported so far.
                let mut turn_usage: Option<crate::providers::base::ProviderUsage> = None;
                // Set by an enforcing loop denial or a non-recoverable provider
                // failure. The terminal event is emitted only after usage and
                // conversation state are durable.
                let mut pending_turn_abort: Option<(TurnAbortCode, String)> = None;

                while let Some(next) = stream.next().await {
                    if is_token_cancelled(&cancel_token) {
                        break;
                    }

                    match next {
                        Ok((response, usage, pending)) => {
                            // A pending tool-call notification: the model has
                            // started a tool block and its name is known, but its
                            // arguments are still generating. Our decoders yield
                            // this in isolation (no message, no usage), so forward
                            // it to the UI and move on. It is intentionally NOT a
                            // `Message`, so it never reaches `categorize_tools`,
                            // `num_tool_requests`, or dispatch — a partial tool
                            // call is structurally incapable of being executed.
                            // (Invariant §6.5.1.)
                            if let Some(pending) = pending {
                                yield AgentEvent::ToolCallPending(pending);
                                continue;
                            }

                            compaction_attempts = 0;
                            // BR-66: the provider is answering again; whatever blip
                            // was retried before is over.
                            mistakes.observe_provider_success();

                            // Emit model change event if provider is lead-worker
                            let provider = self.provider().await?;
                            if let Some(lead_worker) = provider.as_lead_worker() {
                                if let Some(ref usage) = usage {
                                    let active_model = usage.model.clone();
                                    let (lead_model, worker_model) = lead_worker.get_model_info();
                                    let mode = if active_model == lead_model {
                                        "lead"
                                    } else if active_model == worker_model {
                                        "worker"
                                    } else {
                                        "unknown"
                                    };

                                    yield AgentEvent::ModelChange {
                                        model: active_model,
                                        mode: mode.to_string(),
                                    };
                                }
                            }

                            if let Some(ref usage) = usage {
                                if usage.finish_reason.is_some() {
                                    last_finish_reason = usage.finish_reason.clone();
                                }
                                turn_usage = Some(usage.clone());
                            }

                            if let Some(response) = response {
                                made_user_visible_progress |=
                                    message_has_user_visible_progress(&response);
                                ever_made_user_visible_progress |= made_user_visible_progress;
                                // #59: name the reply BEFORE it is yielded, so the
                                // copy the client sees carries the id the row it
                                // becomes is stored under. A provider that stamps
                                // its own streaming `message_id` keeps it (and with
                                // it the same-id merge in `Conversation::push`);
                                // one that does not — `response_to_message` on the
                                // non-streaming path builds a bare
                                // `Message::assistant()` — used to hand the client
                                // `id: null` while the store minted a uid nobody
                                // was ever told.
                                let response = named(response);
                                let ToolCategorizeResult {
                                    frontend_requests,
                                    // BR-19: `mut` — a PreToolUse hook may rewrite a
                                    // tool's input inside inspect_and_gate_tool_requests,
                                    // and the rewritten request is the one dispatched,
                                    // persisted, and handed to the PostToolUse hooks.
                                    mut remaining_requests,
                                    filtered_response,
                                } = self.categorize_tools(&response, &tools).await;

                                yield AgentEvent::Message(filtered_response.clone());
                                tokio::task::yield_now().await;

                                let num_tool_requests = frontend_requests.len() + remaining_requests.len();
                                if num_tool_requests == 0 {
                                    let merged_into_signed_turn =
                                        continues_signed_turn(&response, &messages_to_add)
                                            && response.id.as_deref().is_some_and(|response_id| {
                                                messages_to_add.append_content_to_message_id(
                                                    response_id,
                                                    &response.content,
                                                )
                                            });
                                    if !merged_into_signed_turn {
                                        messages_to_add.push(response.clone());
                                    }
                                    continue;
                                }
                                // Count every tool call this reply requests; the
                                // cumulative total is checked against `max_tool_calls`
                                // at the top of the next iteration.
                                tool_calls_taken = tool_calls_taken.saturating_add(num_tool_requests as u32);

                                // Built in `build_tool_request_maps` rather than
                                // here so this generator's `poll` frame does not
                                // carry the map-building temporaries (issue #87).
                                let ToolBatchMaps {
                                    tool_response_messages,
                                    request_to_response_map,
                                    request_metadata,
                                    request_to_original_tool_call,
                                    mut request_to_executed_tool_call,
                                    request_to_tool_name,
                                } = build_tool_request_maps(
                                    &response,
                                    &frontend_requests,
                                    &remaining_requests,
                                );

                                for (idx, request) in frontend_requests.iter().enumerate() {
                                    let mut frontend_tool_stream = self.handle_frontend_tool_request(
                                        request,
                                        tool_response_messages[idx].clone(),
                                    );

                                    while let Some(msg) = frontend_tool_stream.try_next().await? {
                                        yield AgentEvent::Message(msg);
                                    }
                                    note_tool_argument_rewrite(
                                        &request.id,
                                        &request_to_original_tool_call,
                                        &request_to_executed_tool_call,
                                        &request_to_response_map,
                                    ).await;
                                }
                                // Soft-stage advisories injected after this batch's tool
                                // results so the model can break the loop itself before the
                                // hard stop: BR-29/BR-30's call-shape warnings, gathered from
                                // inspection, plus BR-31's no-progress nudges, gathered from
                                // the results themselves.
                                let mut loop_warnings: Vec<String> = Vec::new();
                                // BR-47: the framed post-edit syntax diagnostics for this
                                // batch, if any. Computed at the result seam but injected
                                // *after* the tool request/response pair below, so the
                                // transcript reads "you edited X (tool response), then the
                                // syntax check on X found ...".
                                let mut pending_post_edit_diagnostics: Option<String> = None;
                                let mut pending_pre_tool_hook_context: Option<Message> = None;
                                let mut pending_post_tool_hook_context: Option<Message> = None;

                                // §6.2c: ids whose tool response was already streamed to the
                                // transcript the moment it completed (in the execution loop
                                // below), in COMPLETION order. The post-batch persistence loop
                                // still pushes every response into `messages_to_add` in REQUEST
                                // order, but skips RE-yielding these — so SQLite keeps request
                                // order while the live transcript surfaces each result as soon
                                // as it lands (invariant §6.5-2). Empty (and the streaming is a
                                // no-op) when `BIOROUTER_TOOL_RESPONSE_STREAMING` is off.
                                let stream_tool_responses =
                                    super::tool_dispatch_limits::tool_response_streaming_enabled();
                                let mut emitted_response_ids: std::collections::HashSet<String> =
                                    std::collections::HashSet::new();

                                if biorouter_mode == BioRouterMode::Chat {
                                    // Skip all remaining tool calls in chat mode
                                    skip_tool_requests_for_chat_mode(
                                        &remaining_requests,
                                        &request_to_response_map,
                                    ).await;
                                } else {
                                    let (
                                        inspection_results,
                                        permission_check_result,
                                        enable_extension_request_ids,
                                        mut tool_futures,
                                    ) = self.inspect_and_gate_tool_requests(
                                        &mut remaining_requests,
                                        &conversation,
                                        biorouter_mode,
                                        &session,
                                        &request_to_response_map,
                                        cancel_token.clone(),
                                    ).await?;
                                    for request in &remaining_requests {
                                        if let Ok(tool_call) = &request.tool_call {
                                            request_to_executed_tool_call
                                                .insert(request.id.clone(), tool_call.clone());
                                        }
                                    }
                                    loop_warnings = crate::tool_inspection::collect_warning_reasons(&inspection_results);

                                    if let Some(abort) = repetition_denial_abort(
                                        &inspection_results,
                                        &permission_check_result,
                                        &mut turn_guard,
                                    ) {
                                        pending_turn_abort = Some(abort);
                                    }

                                    let tool_futures_arc = Arc::new(Mutex::new(tool_futures));

                                    let mut tool_approval_stream = self.handle_approval_tool_requests(
                                        &permission_check_result.needs_approval,
                                        tool_futures_arc.clone(),
                                        &request_to_response_map,
                                        cancel_token.clone(),
                                        &session,
                                        &inspection_results,
                                    );

                                    while let Some(msg) = tool_approval_stream.try_next().await? {
                                        yield AgentEvent::Message(msg);
                                    }

                                    tool_futures = {
                                        let mut futures_lock = tool_futures_arc.lock().await;
                                        futures_lock.drain(..).collect::<Vec<_>>()
                                    };

                                    // BR-19: PreToolUse (inspector) and PermissionRequest
                                    // (permission gate) hooks stage their additionalContext /
                                    // systemMessage there because neither return channel can
                                    // carry them — both sites used to read only the decision
                                    // and drop the rest. Drain them here, once both have run:
                                    // messages surface as inline notices, context reaches the
                                    // model with the same untrusted framing (BR-26) as the
                                    // SessionStart / UserPromptSubmit path.
                                    {
                                        let (notices, context) = staged_tool_hook_context(
                                            self.hooks_manager.drain_tool_hook_context(&session.id),
                                        );
                                        for notice in notices {
                                            yield AgentEvent::Message(notice);
                                        }
                                        if context.is_some() {
                                            pending_pre_tool_hook_context = context;
                                        }
                                    }

                                    let with_id = tool_futures
                                        .into_iter()
                                        .map(|(request_id, stream)| {
                                            stream.map(move |item| (request_id.clone(), item))
                                        })
                                        .collect::<Vec<_>>();

                                    let mut combined = stream::select_all(with_id);
                                    let mut all_install_successful = true;
                                    // (request_id, tool_response, error) captured for PostToolUse hooks
                                    let mut post_tool_results: Vec<(String, Option<Value>, Option<String>)> = Vec::new();

                                    // The cancel token must be raced against the batch
                                    // stream, not merely checked after it yields. Every
                                    // tool in the batch can be slow, and `combined.next()`
                                    // parks until one of them returns — so a check placed
                                    // only after the await cannot observe a cancel until
                                    // some tool finishes, making a cancelled turn wait out
                                    // its slowest call (a 6s child kept a cancelled turn
                                    // alive for the full 6s). Selecting on the token lets
                                    // the loop EXIT at the instant the cancel lands instead
                                    // of blocking on the slowest call; the in-flight tool
                                    // futures live in `combined`, a local that is not
                                    // dropped until the end of this block, so they are torn
                                    // down there rather than literally at the cancel.
                                    // `StreamExt::next` is cancel-safe, so losing the race
                                    // never drops an item that had already resolved.
                                    //
                                    // The same argument covers elicitations (#40): the tool
                                    // call that raises one is parked *inside* `combined`
                                    // until the request is answered or cancelled, so the
                                    // request must also be raced against the batch — the
                                    // old post-item drain could never surface it, and a
                                    // headless run's auto-cancel had to wait out the 300 s
                                    // elicitation timeout.
                                    loop {
                                        let next_item = match next_batch_wake(
                                            &cancel_token,
                                            &mut combined,
                                            &session_config.id,
                                        )
                                        .await
                                        {
                                            BatchWake::Cancelled => None,
                                            BatchWake::ElicitationReady => {
                                                for msg in self
                                                    .drain_elicitation_messages(&session_config.id)
                                                    .await
                                                {
                                                    yield AgentEvent::Message(msg);
                                                }
                                                continue;
                                            }
                                            BatchWake::Item(item) => item,
                                        };
                                        let Some((request_id, item)) = next_item else {
                                            break;
                                        };
                                        if is_token_cancelled(&cancel_token) {
                                            break;
                                        }

                                        for msg in self.drain_elicitation_messages(&session_config.id).await {
                                            yield AgentEvent::Message(msg);
                                        }

                                        match item {
                                            ToolStreamItem::Result(output) => {
                                                self.integrate_tool_result(
                                                    request_id.clone(),
                                                    output,
                                                    &enable_extension_request_ids,
                                                    &request_to_response_map,
                                                    &request_to_tool_name,
                                                    &request_to_original_tool_call,
                                                    &request_to_executed_tool_call,
                                                    &request_metadata,
                                                    &mut all_install_successful,
                                                    &mut post_tool_results,
                                                    tool_output_guardrail,
                                                    tool_error_taxonomy,
                                                ).await;
                                                // §6.2c: emit this tool's response the instant it
                                                // completes, in COMPLETION order, so a slow sibling
                                                // in the same batch can no longer hold a finished
                                                // result off-screen until every tool returns. The
                                                // response mutex now holds the full response
                                                // (`integrate_tool_result` just wrote it). Record
                                                // the id so the post-batch loop persists it in
                                                // request order WITHOUT re-yielding (invariant
                                                // §6.5-2). `select_all` polls in future order, so a
                                                // Result can only surface once its `call_tool`
                                                // resolved — this can never stream a partial.
                                                if stream_tool_responses {
                                                    if let Some(response_msg) =
                                                        request_to_response_map.get(&request_id)
                                                    {
                                                        let response =
                                                            response_msg.lock().await.clone();
                                                        yield AgentEvent::Message(response);
                                                    }
                                                    emitted_response_ids.insert(request_id);
                                                }
                                            }
                                            ToolStreamItem::Message(msg) => {
                                                yield AgentEvent::McpNotification((request_id, msg));
                                            }
                                        }
                                    }

                                    // PAR-04: fill every still-empty slot with an
                                    // explicit "cancelled" result, exactly as chat
                                    // mode does for the calls it skips. The reasoning
                                    // lives on `backfill_cancelled_tool_responses`.
                                    if is_token_cancelled(&cancel_token) {
                                        backfill_cancelled_tool_responses(
                                            &frontend_requests,
                                            &remaining_requests,
                                            &request_to_response_map,
                                        ).await;
                                    }

                                    // BR-47: auto post-edit diagnostics. The collection
                                    // pass and the reflection accounting live in
                                    // `post_edit_file_diagnostics` /
                                    // `post_edit_reflection_text` so this generator's
                                    // `poll` frame does not carry either (issue #87).
                                    if post_edit_diag_config.is_active() {
                                        if let Some(files) = post_edit_file_diagnostics(
                                            &post_tool_results,
                                            &remaining_requests,
                                            &session.working_dir,
                                            &mut post_edit_analyzer,
                                        ) {
                                            pending_post_edit_diagnostics = post_edit_reflection_text(
                                                &session_config.id,
                                                &post_edit_diag_config,
                                                &files,
                                                &mut post_edit_reflections,
                                            );
                                        }
                                    }

                                    // BR-31 + BR-66: the results are in; see
                                    // `extend_batch_nudges` for what each streak
                                    // detector says and why neither blocks.
                                    self.extend_batch_nudges(
                                        &session_config.id,
                                        &conversation,
                                        &remaining_requests,
                                        &permission_check_result,
                                        &request_to_response_map,
                                        &mistake_config,
                                        &mut mistakes,
                                        &mut loop_warnings,
                                    ).await;

                                    // PostToolUse / PostToolUseFailure hooks. The
                                    // dispatch itself lives in
                                    // `dispatch_post_tool_hooks` so this generator's
                                    // `poll` frame does not carry it (issue #87);
                                    // only the yielding half is left here.
                                    {
                                        let hook_outcomes = dispatch_post_tool_hooks(
                                            self.hooks_manager(),
                                            &session_config.id,
                                            &session.working_dir,
                                            post_tool_results,
                                            &remaining_requests,
                                        ).await;
                                        if !hook_outcomes.is_empty() {
                                            let mut hook_contexts: Vec<String> = Vec::new();
                                            let mut blocked_any = false;
                                            for (request_id, tool_name, aggregate) in hook_outcomes {
                                                for msg in &aggregate.system_messages {
                                                    yield AgentEvent::Message(
                                                        inline_notice_user_only(msg.clone(),),
                                                    );
                                                }
                                                if let Some(reason) = aggregate.deny_reason() {
                                                    if self.hooks_manager.note_post_tool_block(&session.id).await {
                                                        blocked_any = true;
                                                        self.apply_post_tool_block(
                                                            &request_id,
                                                            &tool_name,
                                                            reason,
                                                            &request_to_response_map,
                                                        ).await;
                                                        yield AgentEvent::Message(
                                                            inline_notice_user_only(format!("Hook blocked the result of {tool_name}: {reason}"),),
                                                        );
                                                    } else {
                                                        yield AgentEvent::Message(
                                                            inline_notice_user_only(
                                                                format!(
                                                                    "A PostToolUse hook has blocked {tool_name} {} times; delivering the result anyway.",
                                                                    crate::hooks::POST_TOOL_HOOK_BLOCK_CAP
                                                                ),
                                                            ),
                                                        );
                                                    }
                                                }
                                                if let Some(ctx) = aggregate.joined_context() {
                                                    hook_contexts.push(ctx);
                                                }
                                            }
                                            if !blocked_any {
                                                self.hooks_manager.reset_post_tool_blocks(&session.id).await;
                                            }
                                            if let Some(context_message) =
                                                hook_context_message(&hook_contexts)
                                            {
                                                pending_post_tool_hook_context = Some(context_message);
                                            }
                                        }
                                    }

                                    // check for remaining elicitation messages after all tools complete
                                    for msg in self.drain_elicitation_messages(&session_config.id).await {
                                        yield AgentEvent::Message(msg);
                                    }

                                    if all_install_successful && !enable_extension_request_ids.is_empty() {
                                        if let Err(e) = self.save_extension_state(&session_config).await {
                                            warn!("Failed to save extension state after runtime changes: {}", e);
                                        }
                                        tools_updated = true;
                                    }
                                }

                                let signed_provider_turn =
                                    continues_signed_turn(&response, &messages_to_add);

                                if signed_provider_turn {
                                    // A Bedrock reasoning signature authenticates
                                    // the provider-authored assistant block list,
                                    // including original tool arguments and order.
                                    // Execution uses the separate categorized /
                                    // coerced / hook-rewritten ToolRequest values;
                                    // persistence must keep this immutable copy.
                                    // On streaming Bedrock responses the earlier
                                    // reasoning chunk shares this id, so
                                    // Conversation::push reconstructs the one
                                    // signed assistant message before any tool
                                    // result is appended.
                                    let merged = response.id.as_deref().is_some_and(|response_id| {
                                        messages_to_add.append_content_to_message_id(
                                            response_id,
                                            &response.content,
                                        )
                                    });
                                    if !merged {
                                        messages_to_add.push(response.clone());
                                    }

                                    if response.content.iter().any(|content| {
                                        matches!(
                                            content,
                                            MessageContent::ToolRequest(request)
                                                if request.tool_call.is_err()
                                        )
                                    }) {
                                        signed_replay_invalidated_this_iteration = true;
                                        let terminal = "The model ended a signed tool request before its arguments could be preserved safely. BioRouter did not execute the incomplete call and will not send a mutated signed history back to the model. Start a new chat or retry from before this response.".to_string();
                                        let message = named(assistant_text(&terminal));
                                        yield AgentEvent::Message(message.clone());
                                        messages_to_add.push(message);
                                        pending_turn_abort = Some((
                                            TurnAbortCode::SignedReplayInvalidated,
                                            terminal,
                                        ));
                                    }

                                    // The pairing itself is `signed_turn_paired_response_ids`,
                                    // so this generator's `poll` frame does not carry it
                                    // (issue #87); only the yielding half is left here.
                                    for original_id in signed_turn_paired_response_ids(
                                        &response,
                                        &frontend_requests,
                                        &remaining_requests,
                                        &request_to_response_map,
                                    ) {
                                        let Some(response_slot) =
                                            request_to_response_map.get(&original_id)
                                        else {
                                            continue;
                                        };
                                        let final_response = response_slot.lock().await.clone();
                                        if !emitted_response_ids.contains(&original_id) {
                                            yield AgentEvent::Message(final_response.clone());
                                        }
                                        messages_to_add.push(final_response);
                                    }
                                } else {
                                    // The rows themselves are built by
                                    // `unsigned_turn_assistant_rows` (which is also
                                    // where the transcript-shape reasoning lives), so
                                    // this generator's `poll` frame does not carry the
                                    // construction (issue #87).
                                    let (thinking_row, tool_rows) = unsigned_turn_assistant_rows(
                                        &response,
                                        &frontend_requests,
                                        &remaining_requests,
                                    );
                                    if let Some(thinking_row) = thinking_row {
                                        messages_to_add.push(thinking_row);
                                    }
                                    for (idx, request_id, assistant_row) in tool_rows {
                                        messages_to_add.push(assistant_row);
                                        let final_response =
                                            tool_response_messages[idx].lock().await.clone();
                                        if !emitted_response_ids.contains(&request_id) {
                                            yield AgentEvent::Message(final_response.clone());
                                        }
                                        messages_to_add.push(final_response);
                                    }
                                }

                                // The model-only rows this batch staged; see
                                // `push_staged_batch_rows` for what each one is and
                                // why it lands here rather than earlier.
                                push_staged_batch_rows(
                                    &mut messages_to_add,
                                    pending_post_edit_diagnostics.take(),
                                    pending_pre_tool_hook_context.take(),
                                    pending_post_tool_hook_context.take(),
                                    &loop_warnings,
                                );

                                no_tools_called = false;
                                if pending_turn_abort.is_some() {
                                    break;
                                }
                            }
                        }
                        Err(ProviderError::ContextLengthExceeded(_)) => {
                            if signed_replay_context.is_some() {
                                signed_replay_invalidated_this_iteration = true;
                                let terminal = "This chat exceeded the model context window after signed reasoning had been recorded. BioRouter cannot compact or rewrite the authenticated history safely, so it stopped without retrying the model. Start a new chat with the relevant context.".to_string();
                                let message = named(assistant_text(&terminal));
                                yield AgentEvent::Message(message.clone());
                                messages_to_add.push(message);
                                pending_turn_abort = Some((
                                    TurnAbortCode::SignedReplayInvalidated,
                                    terminal,
                                ));
                                break;
                            }
                            compaction_attempts += 1;

                            // BR-13: progressive context-overflow fallback. Instead of a
                            // hard 2-attempt cliff, each successive overflow escalates to a
                            // more aggressive compaction strategy (keep-window ->
                            // shrink-window -> summarize-all -> drop-oldest); only once the
                            // ladder is exhausted do we surface the "still exceeded" error.
                            // `compaction_attempts` resets to 0 on the next successful
                            // provider response, so a later overflow restarts the ladder.
                            let Some(recovery) = overflow_recovery_for_attempt(compaction_attempts) else {
                                error!("Context limit exceeded after progressive compaction fallbacks - prompt too large");
                                yield AgentEvent::Message(
                                    inline_notice("Unable to continue: Context limit still exceeded after compaction. Try using a shorter message, a model with a larger context window, or start a new chat.")
                                );
                                break;
                            };

                            yield AgentEvent::Message(
                                inline_notice("Context limit reached. Compacting to continue the chat...",)
                            );
                            yield AgentEvent::Message(
                                thinking_notice(COMPACTION_THINKING_TEXT,)
                            );

                            self.fire_compaction_hook(
                                crate::hooks::HookEvent::PreCompact,
                                &session_config.id,
                                &session.working_dir,
                                "auto",
                                Some("context_overflow"),
                            );
                            match self.swap_overflow_compaction(
                                &session_config.id,
                                &conversation,
                                &mut rewrite_basis,
                                recovery,
                            ).await {
                                Ok(swap) => {
                                    // BR-35: the round-trips this compaction spent are
                                    // billed by `bill_overflow_compaction`.
                                    self.bill_overflow_compaction(
                                        &session_config,
                                        &swap,
                                        &mut budget,
                                    ).await?;

                                    did_recovery_compact_this_iteration = true;
                                    // PostCompact pairs with the PreCompact above on both
                                    // paths: a compaction did happen and the turn adopts
                                    // it. Only its durability differs.
                                    self.fire_compaction_hook(
                                        crate::hooks::HookEvent::PostCompact,
                                        &session_config.id,
                                        &session.working_dir,
                                        "auto",
                                        Some("context_overflow"),
                                    );
                                    // BR-52: recovery compaction moved the counters too.
                                    if let Some(token_state) = self.current_token_state(&session_config.id).await {
                                        yield AgentEvent::TokenUsage(token_state);
                                    }

                                    match swap.stored {
                                        Some(stored) => {
                                            // The rewrite renumbered every row and minted a
                                            // uid for the summary, so a second overflow in
                                            // this same turn needs a new basis — which the
                                            // swap already re-read AS A PAIR with this
                                            // conversation, before anything above was
                                            // yielded. Refreshing only the revision here
                                            // (with the view fixed at commit time) is what
                                            // let an append landing in between be counted
                                            // by the watermark yet be unknown to the next
                                            // swap, which then deleted it.
                                            yield AgentEvent::HistoryReplaced(stored.clone());
                                            conversation = crate::conversation::without_bedrock_reasoning(&stored);
                                        }
                                        None => {
                                            // Twice declined. Do NOT clobber: keep going
                                            // in memory so the turn can still finish, and
                                            // do not claim the stored history was replaced
                                            // — a reload would then look like it
                                            // resurrected messages.
                                            warn!(
                                                "Overflow-recovery compaction for session {} was not persisted; \
                                                 continuing in memory with the stored history intact",
                                                session_config.id
                                            );
                                            conversation = crate::conversation::without_bedrock_reasoning(&swap.compacted);
                                        }
                                    }
                                    break;
                                }
                                Err(e) => {
                                    error!("Compaction failed: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(ref provider_err) => {
                            error!("Error: {}", provider_err);
                            // BR-66: a non-context provider error used to end the turn
                            // outright, handing the user a "please retry" string for a
                            // blip the agent could have absorbed itself. Give a
                            // *recoverable* error one more attempt with a hint in
                            // context; a fatal one (auth, rate limit, unsupported) or a
                            // spent retry budget still stops, with the conversation
                            // preserved so the user can just say "continue".
                            match mistakes.observe_provider_error(&mistake_config, provider_err) {
                                crate::agents::mistakes::ProviderErrorAction::Recover { notice, attempt, limit } => {
                                    warn!(
                                        "Provider call failed ({provider_err}); retrying with a hint ({attempt}/{limit})"
                                    );
                                    // BR-67: retries are a loop-safety decision too —
                                    // the error text itself never enters the trace.
                                    emit_loop_safety(
                                        LoopSafetyKind::ProviderErrorRecover,
                                        &session_config.id,
                                        attempt,
                                        Some(limit),
                                        None,
                                    );
                                    yield AgentEvent::Message(
                                        inline_notice(format!("Model call failed: {provider_err}. Retrying ({attempt}/{limit})…"),)
                                    );
                                    // Model-visible only: the hint is loop plumbing, and
                                    // the user already has the notification above.
                                    messages_to_add.push(
                                        model_only_user_text_with_new_id(
                                            crate::tool_inspection::frame_loop_warnings(
                                                std::slice::from_ref(&notice),
                                            )
                                        ),
                                    );
                                    did_recover_provider_error_this_iteration = true;
                                    break;
                                }
                                crate::agents::mistakes::ProviderErrorAction::Stop { notice } => {
                                    emit_loop_safety(
                                        LoopSafetyKind::ProviderErrorStop,
                                        &session_config.id,
                                        mistakes.provider_errors(),
                                        None,
                                        None,
                                    );
                                    // #59: named before the yield, persisted under
                                    // the same id below.
                                    let message = named(assistant_text(notice));
                                    yield AgentEvent::Message(message.clone());
                                    messages_to_add.push(message);
                                    pending_turn_abort = Some((
                                        TurnAbortCode::ProviderFailure {
                                            kind: provider_err.kind(),
                                        },
                                        provider_err.to_string(),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }

                // Record the turn exactly once, whether the stream finished, was
                // cancelled, or errored out. The provider still processed (and
                // billed) whatever it reported.
                let usage_recorded = self
                    .record_turn_usage(
                        &session_config,
                        turn_usage.take(),
                        &mut budget,
                        &usage_event_key,
                    )
                    .await?;

                // BR-52: the counters just moved — publish them so downstream
                // consumers (the SSE route) can attach a fresh `TokenState` to
                // every event they forward without touching the DB per token.
                if usage_recorded {
                    if let Some(token_state) = self.current_token_state(&session_config.id).await {
                        yield AgentEvent::TokenUsage(token_state);
                    }
                }

                if tools_updated {
                    (tools, toolshim_tools, system_prompt) =
                        self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                }
                let mut exit_chat = false;
                if pending_turn_abort.is_some() {
                    // The typed failure is emitted after this iteration's messages
                    // and usage have been persisted below.
                } else if last_finish_reason.as_deref() == Some("length") {
                        // The provider cut the response off at the output-length
                        // limit (not a natural stop) and the model called no tool,
                        // so the turn is genuinely unfinished. Auto-continue it
                        // instead of ending on a half-written response. The
                        // budget belongs to the whole reply and is never reset by
                        // intervening tool calls or retries.
                        // (Distinct from "the model chose to stop mid-task" — that
                        // is left to the Stop-hook / goal system, not a hard-coded
                        // loop injection; see the note near the top of this file.)
                        match truncation_recovery.observe(made_user_visible_progress) {
                            TruncationRecoveryAction::Continue => {
                                warn!(
                                    "Response truncated by output-length limit (finish_reason=\"length\"); auto-continuing ({}/{}, zero-progress {}/{})",
                                    truncation_recovery.continuations,
                                    MAX_TRUNCATION_CONTINUATIONS,
                                    truncation_recovery.zero_progress_continuations,
                                    MAX_ZERO_PROGRESS_TRUNCATION_CONTINUATIONS
                                );
                                // Internal loop plumbing: persisted and visible
                                // to the model, but never emitted as a user-authored
                                // chat row. MessagesPersisted still publishes its
                                // id and userVisible=false for edit accounting.
                                messages_to_add.push(named(
                                    model_only_user_text(TRUNCATION_CONTINUATION_MESSAGE),
                                ));
                            }
                            TruncationRecoveryAction::Exhausted { zero_progress } => {
                                let terminal = truncation_exhausted_notice(
                                    zero_progress,
                                    ever_made_user_visible_progress,
                                    truncation_recovery.continuations,
                                );
                                let message = named(assistant_text(&terminal));
                                yield AgentEvent::Message(message.clone());
                                messages_to_add.push(message);
                                pending_turn_abort = Some((
                                    TurnAbortCode::OutputRecoveryExhausted {
                                        continuations: truncation_recovery.continuations,
                                        zero_progress,
                                    },
                                    terminal,
                                ));
                            }
                        }
                } else if no_tools_called {
                    // Observability: a turn that ends without a tool call is either a
                    // natural completion ("stop") or an unreported end (None).
                    info!(
                        "turn ended with no tool call; finish_reason={:?}",
                        last_finish_reason
                    );
                    let final_output_state = self
                        .final_output_tool
                        .lock()
                        .await
                        .as_ref()
                        .map(|tool| tool.final_output.clone());
                    if let Some(final_output) = final_output_state {
                        match final_output {
                            None => {
                                warn!("Final output tool has not been called yet. Continuing agent loop.");
                                let message = named(Message::user().with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE));
                                messages_to_add.push(message.clone());
                                yield AgentEvent::Message(message);
                            }
                            Some(final_output) => {
                                let message = named(assistant_text(final_output));
                                messages_to_add.push(message.clone());
                                yield AgentEvent::Message(message);
                                exit_chat = true;
                            }
                        }
                    } else if did_recovery_compact_this_iteration {
                        // Avoid setting exit_chat; continue from last user message in the conversation
                    } else if did_recover_provider_error_this_iteration {
                        // BR-66: the provider call failed recoverably and the hint is
                        // already in `messages_to_add`. No tool ran and the model said
                        // nothing, so this is not a finished turn — take the retry
                        // rather than ending the turn (or handing it to the retry
                        // manager, whose job is a *completed* response that failed
                        // validation). Bounded by `provider_error_retries`, and by
                        // `max_turns` like every other iteration.
                    } else {
                        match self.handle_retry_logic(&mut conversation, &session_config, initial_messages.messages()).await {
                            Ok(should_retry) => {
                                if should_retry {
                                    info!("Retry logic triggered, restarting agent loop");
                                    did_retry_reset_this_iteration = true;
                                } else {
                                    exit_chat = true;
                                }
                            }
                            Err(e) => {
                                error!("Retry logic failed: {}", e);
                                yield AgentEvent::Message(
                                    assistant_text(format!("Retry logic encountered an error: {}", e))
                                );
                                exit_chat = true;
                            }
                        }
                    }
                }

                // #41 / the uid adoption both live on
                // `persist_iteration_messages`.
                let messages_to_add = persist_iteration_messages(
                    &session_manager,
                    &session_config.id,
                    messages_to_add,
                ).await?;
                // #59: this iteration's rows are durable — publish the uids they
                // actually took. This is the one site that closes the gaps a
                // yielded copy structurally cannot: a re-mint on collision, the
                // rebuilt thinking / tool-request rows one streamed reply is
                // split into (only the first keeps the reply's id), and the
                // model-only rows the user is deliberately never shown (BR-47
                // post-edit diagnostics, loop-guard nudges, hook context).
                //
                // #66 SHAPE 3: the batch is mixed — the streamed reply and its
                // tool requests were yielded earlier in this same iteration,
                // the model-only rows never will be. Shape 3 is the honest
                // label for the batch as a whole, because it is the one that
                // carries an ordering obligation, and it is already discharged:
                // every yield above precedes this line.
                if let Some(published) = named_after_earlier_yield(messages_to_add.iter()) {
                    yield published;
                }
                fold_iteration_into_conversation(
                    &mut conversation,
                    &mut signed_replay_context,
                    &conversation_with_moim,
                    messages_to_add,
                    did_retry_reset_this_iteration,
                    signed_replay_invalidated_this_iteration,
                );
                if exit_chat
                    && pending_turn_abort.is_none()
                    && signed_replay_context.is_some()
                {
                    conversation = crate::conversation::without_bedrock_reasoning(&conversation);
                    signed_replay_context = None;
                }

                // BR-28: turn boundary — join the observe-only hooks fired during
                // this iteration (Notification on a permission prompt, Pre/PostCompact
                // on an in-loop compaction) and surface what they returned instead of
                // dropping their aggregate. Placed before the `exit_chat` branch so
                // every exit path passes through it.
                for msg in self.settle_fired_hooks(&session_config.id).await {
                    yield AgentEvent::Message(msg);
                }

                if !no_tools_called {
                    // Tools ran this iteration: any Stop-hook block streak is over.
                    // Output-recovery budgets intentionally do not reset here.
                    self.hooks_manager.reset_stop_blocks(&session_config.id).await;

                    // BR-43: post-step snapshot of the (possibly mutated) work-tree.
                    // Coarse: any tool-running iteration, relying on the shadow
                    // repo's tree-sha dedup to drop read-only steps (no row when the
                    // tree is unchanged).
                    self.maybe_checkpoint(
                        &session_config.id,
                        &working_dir,
                        checkpoint_anchor_ts,
                        CheckpointKind::PostStep,
                    ).await;
                }

                if let Some((code, message)) = pending_turn_abort.take() {
                    yield AgentEvent::TurnAborted { code, message };
                    break;
                }

                // BR-61 + #69: take-and-close atomically. A non-empty drain keeps
                // the loop alive for one more step so the steer is answered in
                // context (BR-61: it would otherwise sit parked until some later
                // turn injected it out of nowhere); an empty drain closes the
                // queue, so anything arriving afterwards is refused rather than
                // stranded. The two must be one critical section — a separate
                // `has_soft_interrupts` check followed by an exit is exactly the
                // window #69 reports. Still bounded by max_turns / max_tool_calls,
                // which are re-checked at the top.
                if exit_chat {
                    match self.close_and_drain() {
                        Drained::Some(pending) => {
                            info!(
                                count = pending.len(),
                                "soft interrupt pending at turn exit; continuing the loop to consume it"
                            );
                            self.requeue_for_this_turn(pending);
                            exit_chat = false;
                        }
                        Drained::Empty => {}
                    }
                }

                if exit_chat {
                    if session.session_type == SessionType::SubAgent {
                        // Subagents get an observe-only SubagentStop instead of a
                        // blockable Stop (avoids nested runaway loops).
                        let mut payload = crate::hooks::HookPayload::new(
                            crate::hooks::HookEvent::SubagentStop,
                            &session_config.id,
                            session.working_dir.to_string_lossy(),
                        );
                        payload.subagent_id = Some(session_config.id.clone());
                        self.hooks_manager.fire(
                            crate::hooks::HookEvent::SubagentStop,
                            None,
                            payload,
                            session.working_dir.clone(),
                        );
                        // BR-28: this break is the subagent's last boundary, so settle
                        // the SubagentStop hook here — nothing downstream would ever
                        // join it, and its aggregate would be lost with the task.
                        for msg in self.settle_fired_hooks(&session_config.id).await {
                            yield AgentEvent::Message(msg);
                        }
                        break;
                    }
                    let active_goal = self.active_goal(&session_config.id).await;

                    // BR-48: deterministic done-ness gate. When enabled, re-run
                    // the configured `SuccessCheck`s before the turn is allowed to
                    // finish; on failure inject *what failed* and keep working
                    // (iterating on the current diff, never resetting the way the
                    // workflow retry does). Skipped when the turn is already being
                    // wound down under a stall/budget deadline or after a cancel —
                    // those wrap-ups must be allowed to end. Default OFF, so this
                    // is inert unless a user opted in. Runs before the (optional,
                    // LLM) self-critique so a broken build is caught deterministically
                    // and cheaply, without spending a judge call.
                    if done_gate_config.is_active()
                        && stall_deadline.is_none()
                        && budget_deadline.is_none()
                        && !is_token_cancelled(&cancel_token)
                    {
                        let failures = crate::agents::retry::collect_check_failures(
                            &done_gate_config.checks,
                            done_gate_config.timeout,
                            Some(working_dir.as_path()),
                        )
                        .await;
                        if !failures.is_empty() {
                            if done_gate_iterations < done_gate_config.max_iterations {
                                done_gate_iterations += 1;
                                emit_loop_safety(
                                    LoopSafetyKind::DoneGateBlock,
                                    &session_config.id,
                                    done_gate_iterations,
                                    Some(done_gate_config.max_iterations),
                                    None,
                                );
                                // #59 / #66 SHAPE 2: hidden from the user, named
                                // for the client.
                                let (feedback, published) = persist_steering_message(
                                    &session_manager,
                                    &session_config.id,
                                    crate::agents::done_gate::gate_instruction(&failures),
                                ).await?;
                                if let Some(published) = published {
                                    yield published;
                                }
                                conversation.push(feedback);
                                // Keep looping so the model fixes the failures;
                                // skip this iteration's Stop hook. The counter does
                                // not reset on the tool calls the fix requires, so
                                // the loop is bounded by `max_iterations`.
                                //
                                // #69: the exit above already closed the queue on
                                // the assumption the turn was over. It is not, so
                                // re-open it — refusing steers for the rest of a
                                // turn that is still working would be its own bug.
                                self.reopen_for_more_work();
                                tokio::task::yield_now().await;
                                continue;
                            } else {
                                // Budget spent with checks still red: let the turn
                                // finish rather than wedge, but tell the user it is
                                // on unmet conditions.
                                emit_loop_safety(
                                    LoopSafetyKind::DoneGateGiveUp,
                                    &session_config.id,
                                    done_gate_iterations,
                                    Some(done_gate_config.max_iterations),
                                    None,
                                );
                                yield AgentEvent::Message(
                                    inline_notice_user_only(
                                        crate::agents::done_gate::giveup_notice(
                                            done_gate_iterations,
                                            &failures,
                                        ),
                                    ),
                                );
                            }
                        }
                    }

                    // BR-50: optional self-critique pass on an *ordinary* answer.
                    // Skipped when a /goal is active (its Stop-hook judge already
                    // re-reads the work), when the turn is already being wrapped up
                    // under a stall/budget deadline (a critique would re-expand a
                    // turn we are deliberately ending), when cancelled, or once the
                    // per-reply pass budget is spent. Default OFF, so this is inert
                    // unless a user opted in.
                    if self_critique_config.is_active()
                        && active_goal.is_none()
                        && stall_deadline.is_none()
                        && budget_deadline.is_none()
                        && self_critique_passes < self_critique_config.max_passes
                        && !is_token_cancelled(&cancel_token)
                    {
                        if let Some(reason) = self.run_self_critique(&conversation).await {
                            self_critique_passes += 1;
                            emit_loop_safety(
                                LoopSafetyKind::SelfCritiqueRevise,
                                &session_config.id,
                                self_critique_passes,
                                None,
                                None,
                            );
                            // #59 / #66 SHAPE 2: hidden from the user, named for
                            // the client.
                            let (feedback, published) = persist_steering_message(
                                &session_manager,
                                &session_config.id,
                                crate::agents::self_critique::revise_instruction(&reason),
                            ).await?;
                            if let Some(published) = published {
                                yield published;
                            }
                            conversation.push(feedback);
                            // Keep looping so the model revises; skip this
                            // iteration's Stop hook. The next finish attempt runs
                            // Stop hooks normally, and the critique won't fire again
                            // once the pass budget is spent.
                            //
                            // #69: re-open the queue the exit check closed — the
                            // turn is continuing after all.
                            self.reopen_for_more_work();
                            tokio::task::yield_now().await;
                            continue;
                        }
                    }

                    let transcript_tail = crate::agents::goal::transcript_tail(&conversation);
                    match self.hooks_manager.stop(&session_config.id, &session.working_dir, transcript_tail).await {
                        crate::hooks::StopHookVerdict::Proceed => {
                            // An active goal whose evaluator let the stop
                            // proceed is met: clear it and tell the user.
                            if let Some(goal) = active_goal {
                                self.clear_goal(&session_config.id).await;
                                yield AgentEvent::Message(
                                    inline_notice_user_only(
                                        format!(
                                            "🎯 Goal met and cleared: {}",
                                            crate::agents::goal::ellipsize(&goal.condition, 200)
                                        ),
                                    ),
                                );
                            }
                            break;
                        }
                        crate::hooks::StopHookVerdict::CapReached => {
                            let goal_hint = if active_goal.is_some() {
                                " The /goal stays active and will be re-evaluated next turn; run /goal clear to stop it."
                            } else {
                                ""
                            };
                            yield AgentEvent::Message(
                                inline_notice_user_only(
                                    format!(
                                        "Stop hook block limit ({}) reached; finishing anyway.{}",
                                        crate::hooks::STOP_HOOK_BLOCK_CAP,
                                        goal_hint
                                    ),
                                ),
                            );
                            break;
                        }
                        crate::hooks::StopHookVerdict::Blocked { reason } => {
                            // The goal-budget accounting lives on
                            // `stop_hook_block_feedback`.
                            let (feedback_text, notice) = self.stop_hook_block_feedback(
                                &session_config.id,
                                &reason,
                                active_goal.is_some(),
                            ).await;

                            // #59 / #66 SHAPE 2: hidden from the user, named for
                            // the client.
                            let (feedback, published) = persist_steering_message(
                                &session_manager,
                                &session_config.id,
                                feedback_text,
                            ).await?;
                            if let Some(published) = published {
                                yield published;
                            }
                            conversation.push(feedback);
                            yield AgentEvent::Message(
                                inline_notice_user_only(notice,),
                            );
                            // Keep looping: the model sees the feedback next turn.
                            // After a give-up the goal is cleared, so the next stop
                            // proceeds once the agent delivers its wrap-up.
                            //
                            // #69: a blocked Stop reverses the exit the queue was
                            // closed for, so re-open it for the extra work.
                            self.reopen_for_more_work();
                        }
                    }
                }

                tokio::task::yield_now().await;
            }

            // #69: the loop is over on every path that reaches here — including
            // the aborts that break out above `close_and_drain` (cancel, budget,
            // stall, max_turns). Close the queue so a steer aimed at this turn is
            // refused rather than accepted into a session with nothing running.
            // A steer that got in first is reported (it is about to be dropped by
            // the next `open_for_turn`); it cannot be answered, because there is
            // no loop left to answer it. An early-cancelled consumer can drop this
            // stream before this line, in which case the next turn's
            // `open_for_turn` is what clears the queue.
            if let Drained::Some(stranded) = self.close_and_drain() {
                warn!(
                    count = stranded.len(),
                    turn = %this_turn,
                    "turn ended before its queued soft interrupts could be injected"
                );
                // Closed for good: re-taking the (now empty) queue flips
                // `accepting` off, which the non-empty branch above left on.
                let _ = self.close_and_drain();
            }

            // BR-12: the turn is complete — the agent loop drained and control is
            // returning to the user. If the session ended over the compaction
            // threshold, kick off compaction in the background now, *between*
            // turns, so the next turn starts from an already-compacted history
            // instead of stalling on the summarization round-trip. Fire-and-forget
            // (spawns a task, doesn't await it); the synchronous check at the top
            // of reply() is the fallback if this hasn't landed by then. Like the
            // rename below, this tail runs only when the consumer drains the stream
            // to completion — an early cancel just defers compaction to that
            // synchronous fallback, which is harmless.
            self.maybe_spawn_eager_compaction(&session_config, &working_dir);

            // NOTE: LLM-driven session rename is intentionally NOT triggered here.
            // This code sits after the last `yield` of a lazy `async_stream`, so it
            // only runs if the consumer drains the stream all the way to `None`.
            // The SSE consumer can `break` early (e.g. on client disconnect /
            // cancellation) before that final poll, in which case the stream future
            // is dropped and this tail never executes — leaving the session stuck on
            // "New chat". The rename is now driven by the consumer instead, via
            // `maybe_rename_session`, which is guaranteed to run after the reply loop
            // ends regardless of how it ended. See routes/reply.rs and routes/apps.rs.
        }))
    }

    /// Best-effort LLM session rename, safe to call after a reply loop ends.
    ///
    /// Consumers of `reply()` call this once the stream loop exits (normal end,
    /// error, or cancellation). Unlike a tail appended to the lazy reply stream,
    /// this always runs, so a session with a real exchange is never left as the
    /// "New chat" placeholder. `maybe_update_name` is itself idempotent and
    /// guarded (it skips user-named sessions and stops after the first few
    /// exchanges), so calling it once per reply is cheap and correct.
    pub async fn maybe_rename_session(&self, session_id: &str) {
        let provider = match self.provider().await {
            Ok(provider) => provider,
            Err(e) => {
                warn!("Skipping session rename, no provider available: {}", e);
                return;
            }
        };
        if let Err(e) = self
            .config
            .session_manager
            .maybe_update_name(session_id, provider)
            .await
        {
            warn!("Failed to generate session description: {}", e);
        }
    }

    pub async fn extend_system_prompt(&self, instruction: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.add_system_prompt_extra(instruction);
    }

    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();
        let model_config = provider.get_model_config();
        let tier = provider.tier();
        let model_config_json = serde_json::to_string(&model_config)
            .context("Failed to serialize the provider's model config")?;

        // Issue #56 Gate A. Persist FIRST: the in-memory swap used to precede
        // the persist, so a refused write would leave the chat running on the
        // refused model. The invariant this establishes is one sentence, and it
        // is narrower than it looks: **a bind is never accepted against a row
        // that is already private.** NOT "the provider bound to a private
        // session is always private" — a ratchet that commits after a legal
        // bind produces (private, public provider), and that residual is Gate
        // B's, not this gate's.
        //
        // ⚠ There is NO seam at this line. The before-write rendezvous lives
        // inside the storage helper called below, between any read that function
        // does and the statement that writes — see its doc comment. A seam here
        // parks before the helper is entered and therefore cannot tell a
        // conditional UPDATE from a SELECT-then-UPDATE, which is the exact
        // implementation Gate A exists to reject.
        match self
            .config
            .session_manager
            .storage()
            .bind_provider_if_allowed(
                session_id,
                &provider_name,
                &model_config_json,
                tier.is_private(),
            )
            .await
            .context("Failed to persist provider config to session")?
        {
            BindOutcome::Bound => {
                // Issue #56 Gate B'. A bind Gate A's `WHERE` clause ACCEPTED is
                // an observation about the row, and until this agent's next
                // `reply` it is the only one it has: `AND (privacy_tier =
                // 'public' OR ? = 1)` admits a PUBLIC provider only when the
                // row is public. So a successful public bind *reads* the
                // classification off the gate's own outcome — it does not
                // derive one from a tier, which is what `privacy::floor` is for
                // and why this is not a second crossing between the lattices.
                //
                // Without it the cache stays at its fail-closed `Private`
                // default and `provider()` refuses the very provider that was
                // just legally bound — which is a startup failure on every
                // fresh public session (the CLI asserts `provider()` before its
                // first turn), not a privacy control.
                //
                // A successful PRIVATE bind teaches nothing: it is admitted
                // against either classification. The cache is left alone, and a
                // private provider satisfies Gate B' whatever it says.
                //
                // ⚠ DR-15's master opt-out belongs in this condition, and it is
                // not a refusal — it is what keeps the sentence above TRUE. With
                // the toggle off the `WHERE` clause admits every bind, so an
                // accepted public bind is no longer an observation about the
                // row, and storing `Public` would leave the cache asserting
                // something the gate never checked. Turning the feature back on
                // would then hand Gate B' a stale `Public` for a row that is
                // still marked private, until the next `reply` re-read it.
                // Skipping the store instead leaves the fail-closed `Private`
                // default, which `reply` repairs on the next turn.
                if !tier.is_private() && crate::privacy::privacy_tiers_enabled() {
                    self.cached_classification
                        .store(session_id, SessionClassification::Public);
                }
            }
            BindOutcome::RefusedByPrivacy => {
                return Err(PrivacyRefusal::PublicModelOnPrivateSession {
                    session_id: session_id.to_string(),
                    provider: provider_name,
                }
                .into());
            }
            // ⚠ DEVIATION FROM THE PLAN, DELIBERATE. The plan returned
            // `Err(anyhow!("No such session"))` here. That is not this task's
            // change to make: an `UPDATE` matching no row has never been an
            // error in this tree, and four existing tests depend on it — most
            // explicitly `a_run_that_panics_before_the_stream_still_closes_its_bracket`
            // (`subagent_handler.rs`), whose entire premise is that the two `?`
            // exits in the bracket window "cannot be made to fail for any input
            // a caller controls" *because* an UPDATE matching no row is not an
            // error. Turning a silent no-op into a hard error is an unrelated
            // behaviour change with a blast radius no test in this plan covers.
            //
            // The distinction the three-way outcome exists for is untouched and
            // is what matters: an id that names no row must NEVER be reported
            // as a privacy refusal, because a stale or mistyped id would then
            // reach the user as "this chat is private". A row that does not
            // exist has no classification to violate, so there is nothing to
            // refuse — the persist is skipped and the in-memory swap proceeds,
            // exactly as before #56.
            BindOutcome::NoSuchSession => {
                tracing::warn!(
                    session_id,
                    provider = provider_name,
                    "bound a provider in memory for a session with no row; nothing persisted"
                );
            }
        }

        #[cfg(test)]
        seams::after_bind_before_swap().await;
        {
            let mut current_provider = self.provider.lock().await;
            *current_provider = Some(provider);
        }

        // Issue #56 Task 48, DR-26's third axis at the BIND surface. Binding a
        // model covered by one institution's agreements into a chat holding
        // another institution's connectors is the same mismatch the enable path
        // sees from the other end, and it is discovered here first because the
        // extensions were already attached.
        //
        // ⚠ **It warns; it does not refuse, and it must not.** Gate A above
        // refuses a bind on the TIER axis because a public model in a private
        // chat is a capability the row forbids. Affiliation is not that: both
        // endpoints are Private, legitimate cross-institutional work under a
        // real DUA exists, and a blocked-outright design is one researchers
        // route around by turning the feature off (DR-19, DR-26). Refusing here
        // would also strand a chat — the model is bound, so the only exit would
        // be removing extensions the user cannot see the reason for.
        //
        // The log is not the product — it is the support transcript's copy of a
        // statement the user gets separately. DR-26's user-facing statement at a
        // bind is [`Self::cross_affiliation_notice`], which wraps the same query
        // this loop reads and which `POST /agent/update_provider` returns in its
        // 200 body for the model picker to show. Before that existed this loop
        // WAS the whole of the bind surface's DR-26 story, and a user watching
        // the screen was told nothing until they tried to use the connector.
        //
        // ⚠ **This loop must stay a log and must not become the surface.** It
        // runs inside `update_provider`, which every non-HTTP bind path also
        // calls — the CLI's `configure`/`web`/session builder, the scheduler,
        // ACP, the apps runtime — and none of those has a user watching. The
        // statement is *pulled* by the surface that has one, which is why the
        // notice is a method rather than a side effect here.
        //
        // ⚠ On the RESTART path this can legitimately log nothing:
        // `restore_provider_from_session` is `tokio::join!`ed with
        // `load_extensions_from_session`, so the extension set may still be
        // filling when this runs. That is why the query — not this loop — is
        // what a surface asks; a log line that raced is a missing log line, and
        // every gate that refuses reads the set at the moment it decides.
        for (extension, warning) in self.cross_affiliation_warnings().await {
            tracing::warn!(session_id, provider = provider_name, extension, "{warning}");
        }
        Ok(())
    }

    /// Every enabled extension the model bound **right now** is affiliation-
    /// incompatible with, as `(extension key, the warning)` — DR-26.
    ///
    /// This is the bind surface's user-facing half: "this model is incompatible
    /// with N enabled extensions", stated specifically enough to act on. It
    /// decides nothing and blocks nothing; a mismatch warns and asks, and the
    /// user may proceed.
    ///
    /// Empty is the normal answer — for every public model (the tier gates own
    /// those), for every local model (`Local` reaches everything private,
    /// because no transfer occurs at all), and for a model bound to the same
    /// institution as the extensions it can see.
    ///
    /// ⚠ **Empty in `open`, too** (issue #56 Task 52, DR-27), through
    /// [`crate::privacy::affiliation::refusing_mismatches`] — the ONE place that
    /// setting is read. This is the sentence a user is shown so they can decide
    /// whether to proceed, and DR-27's `open` is *allowed silently*; the log line
    /// at the bind and `/agent/add_extension`'s are the same statement from the
    /// two ends, so they must not disagree about whether to speak.
    ///
    /// ⚠ **The RESOLUTION behind it is not narrowed and must not be.** DR-27
    /// requires the compatibility result to stay computed and available, so
    /// `ExtensionManager::extension_reach` still marks in `open`, and
    /// [`Self::cross_affiliation_grant_subject`] — which reads the extension
    /// manager's list directly rather than through this method — still finds its
    /// subject in all three modes.
    pub async fn cross_affiliation_warnings(&self) -> Vec<(String, String)> {
        crate::privacy::affiliation::refusing_mismatches(
            self.extension_manager
                .cross_affiliation_warnings(None)
                .await,
        )
    }

    /// What separates one warning from the next in
    /// [`Self::cross_affiliation_notice`]'s body.
    ///
    /// ⚠ **A wire detail, mirrored in `ui/desktop/src/utils/crossAffiliation.ts`
    /// as `CROSS_AFFILIATION_NOTICE_SEPARATOR`.** A blank line rather than a
    /// newline, because each warning is a full sentence naming two institutions
    /// and a run-together pair reads as one confused claim about three.
    pub const CROSS_AFFILIATION_NOTICE_SEPARATOR: &'static str = "\n\n";

    /// DR-26's statement for the two surfaces that **warn and proceed** — the
    /// bind (`POST /agent/update_provider`) and the user's own enable
    /// (`POST /agent/add_extension`) — as one body they return to the person who
    /// just acted. Empty means there is nothing to say.
    ///
    /// ⚠ **This exists because the ruling was log-only where it mattered.**
    /// [`Self::cross_affiliation_warnings`] has been correct since Task 48 and
    /// was read by nothing a user could see: `update_provider`'s `tracing::warn!`
    /// loop and `/agent/add_extension`'s were the only callers, so a researcher
    /// enabling another institution's connector from Settings was told nothing at
    /// all. The gate was fine; the sentence never left the daemon.
    ///
    /// ⚠ **It removes flows the user has already accepted, and `model` is what
    /// makes that safe.** A grant is keyed on (session, extension, model
    /// affiliation), so the affiliation passed here has to be the one the caller
    /// actually bound or attached against — never a fresh sample. Both callers
    /// hold the authoritative value: `update_provider` holds the provider it just
    /// created, `add_extension` the one it read once for both privacy axes. A
    /// re-read here would be the read-then-read `CallCapability` exists to
    /// collapse, and getting it wrong in the permissive direction means
    /// suppressing a warning against the wrong institution's acceptance.
    ///
    /// ⚠ **Suppression is narrow, and the narrowness is the point.** A bind to a
    /// *different* institution's model produces a different key, so no earlier
    /// acceptance can cover it and the user is asked again — which is DR-26's
    /// intent, not a redundancy. What is suppressed is only the case where the
    /// user has already said yes to this exact triple at a dispatch, where
    /// repeating the warning would state a boundary the daemon has already agreed
    /// to let them cross.
    ///
    /// ⚠ **Fail-loud, not fail-quiet.** [`crate::privacy::grant::is_granted`]
    /// answers `false` for an unreadable store, so a database hiccup makes this
    /// warn where it might not have needed to. That is the only acceptable
    /// direction: the opposite would silently withhold a privacy statement.
    ///
    /// ⚠ **The one window this does NOT close, written down rather than left to
    /// be discovered.** The warnings come from
    /// [`Self::cross_affiliation_warnings`], which samples the provider mutex
    /// itself, while `model` was read by the caller earlier in its own handler.
    /// A concurrent `update_provider` on the SAME chat between those two reads
    /// would have this suppress a warning about the newly bound model on an
    /// acceptance recorded for the old one — and that direction fails OPEN, so it
    /// is not a nicety. Closing it needs the warnings and the affiliation to come
    /// off one [`crate::privacy::CallCapability`], the way
    /// [`Self::cross_affiliation_grant_subject`] does; that is a change inside
    /// `ExtensionManager::cross_affiliation_warnings`, not here. What bounds it
    /// meanwhile: both callers are user actions on one chat, which the GUI
    /// serialises, and the residue is one unshown warning for a connector the
    /// same user already accepted under a neighbouring model.
    pub async fn cross_affiliation_notice(
        &self,
        session_id: &str,
        model: Option<crate::privacy::ModelAffiliation>,
    ) -> String {
        let mut speak: Vec<String> = Vec::new();
        for (extension, warning) in self.cross_affiliation_warnings().await {
            if crate::privacy::grant::is_granted(
                &self.config.session_manager,
                session_id,
                &extension,
                model,
            )
            .await
            {
                continue;
            }
            speak.push(warning);
        }
        speak.join(Self::CROSS_AFFILIATION_NOTICE_SEPARATOR)
    }

    /// Task 49 (DR-26): everything the grant route needs about ONE extension,
    /// from ONE sample — the warning the user is being asked to accept, and the
    /// model affiliation the grant will be keyed on.
    ///
    /// `None` means there is nothing to accept: the extension is not enabled in
    /// this chat, or the model bound right now is compatible with it. The route
    /// turns that into a refusal rather than recording a grant, because DR-26's
    /// whole premise is that a user accepts a risk **that was stated to them** —
    /// a grant with no live mismatch behind it is a pre-authorisation for a flow
    /// nobody has described yet.
    ///
    /// ⚠ **The two values come from one `CallCapability`, and that is the point
    /// of the method existing at all.** `Agent::update_provider` reassigns the
    /// provider mutex with no turn lock, so asking for the warning and then
    /// asking for the affiliation is the read-then-read that type exists to
    /// collapse: the user would be shown institution A's statement and the grant
    /// would be recorded against institution B's model — an acceptance of a
    /// sentence the user never read.
    pub async fn cross_affiliation_grant_subject(
        &self,
        extension: &str,
    ) -> Option<(Option<crate::privacy::ModelAffiliation>, String)> {
        let cap = crate::privacy::CallCapability::sample(&self.provider).await;
        let key = crate::config::extensions::name_to_key(extension);
        self.extension_manager
            .cross_affiliation_warnings(Some(cap))
            .await
            .into_iter()
            .find(|(name, _)| crate::config::extensions::name_to_key(name) == key)
            .map(|(_, warning)| (cap.affiliation(), warning))
    }

    /// Restore the provider from session data or fall back to global config
    /// This is used when resuming a session to restore the provider state
    pub async fn restore_provider_from_session(&self, session: &Session) -> Result<()> {
        let config = Config::global();

        let provider_name = session
            .provider_name
            .clone()
            .or_else(|| config.get_biorouter_provider().ok())
            .ok_or_else(|| anyhow!("Could not configure agent: missing provider"))?;

        let model_config = match session.model_config.clone() {
            Some(saved_config) => saved_config,
            None => {
                let model_name = config
                    .get_biorouter_model()
                    .map_err(|_| anyhow!("Could not configure agent: missing model"))?;
                crate::model::ModelConfig::new(&model_name)
                    .map_err(|e| anyhow!("Could not configure agent: invalid model {}", e))?
            }
        };

        let provider = crate::providers::create(&provider_name, model_config)
            .await
            .map_err(|e| anyhow!("Could not create provider: {}", e))?;

        // ⚠ Issue #56. This re-binds the row's OWN recorded provider, and Gate A
        // can refuse it: a row that is (private, public `provider_name`) makes
        // this return `PrivacyRefusal`, which `?`-propagates into resume,
        // restart and the injected-turn path, leaving the chat UNOPENABLE
        // rather than repairable.
        //
        // ⚠ This comment used to say "unreachable today: nothing in production
        // writes `privacy_tier = 'private'` yet", and addressed itself to Gate
        // B's task. Gate B has landed — its ratchet in `reply` is now exactly
        // that writer — so the premise is gone and the note is corrected rather
        // than deleted. Gate B deliberately did NOT change this site, for a
        // reason worth writing down: **Gate B's repair does not apply here.**
        // That repair works by rebinding the provider the ROW names, which
        // helps only when the live agent is holding something else. This site
        // is already binding the row's own provider, so when the row itself is
        // the inconsistent pair there is nothing to rebind to and no repair a
        // rebind can perform.
        //
        // Reachability today is narrow but no longer nil. Every in-process
        // sequence keeps the row's `provider_name` in step with the tier — the
        // ratchet only fires with a private provider bound, and every bind path
        // writes the row and the binding together — so producing the pair takes
        // a row edited outside this agent's lifetime: a second process on the
        // same session, a row restored from a backup, or a `provider_name` whose
        // tier changed in the catalog under a session already ratcheted.
        //
        // Swallowing the refusal here is NOT the fix — it would run a public
        // model against a private session, which is the one thing Gate A exists
        // to stop. The fix is the repair card reaching this site and not only
        // `reply`, which needs a UI surface that does not exist yet.
        self.update_provider(provider, &session.id).await
    }

    /// Override the system prompt with a custom template
    pub async fn override_system_prompt(&self, template: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.set_system_prompt_override(template);
    }

    pub async fn list_extension_prompts(&self) -> HashMap<String, Vec<Prompt>> {
        self.extension_manager
            .list_prompts(CancellationToken::default())
            .await
            .expect("Failed to list prompts")
    }

    pub async fn get_prompt(&self, name: &str, arguments: Value) -> Result<GetPromptResult> {
        // First find which extension has this prompt
        let prompts = self
            .extension_manager
            .list_prompts(CancellationToken::default())
            .await
            .map_err(|e| anyhow!("Failed to list prompts: {}", e))?;

        if let Some(extension) = prompts
            .iter()
            .find(|(_, prompt_list)| prompt_list.iter().any(|p| p.name == name))
            .map(|(extension, _)| extension)
        {
            return self
                .extension_manager
                .get_prompt(extension, name, arguments, CancellationToken::default())
                .await
                .map_err(|e| anyhow!("Failed to get prompt: {}", e));
        }

        Err(anyhow!("Prompt '{}' not found", name))
    }

    pub async fn get_plan_prompt(&self) -> Result<String> {
        let tools = self.extension_manager.get_prefixed_tools(None).await?;
        let tools_info = tools
            .into_iter()
            .map(|tool| {
                ToolInfo::new(
                    &tool.name,
                    tool.description
                        .as_ref()
                        .map(|d| d.as_ref())
                        .unwrap_or_default(),
                    get_parameter_names(&tool),
                    None,
                )
            })
            .collect();

        let plan_prompt = self.extension_manager.get_planning_prompt(tools_info).await;

        Ok(plan_prompt)
    }

    pub async fn handle_tool_result(&self, id: String, result: ToolResult<CallToolResult>) {
        if let Err(e) = self.tool_result_tx.send((id, result)).await {
            error!("Failed to send tool result: {}", e);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn create_workflow(&self, mut messages: Conversation) -> Result<Workflow> {
        tracing::info!(
            "Starting workflow creation with {} messages",
            messages.len()
        );

        let extensions_info = self.extension_manager.get_extensions_info().await;
        tracing::debug!("Retrieved {} extensions info", extensions_info.len());

        // Get model name from provider
        let provider = self.provider().await.map_err(|e| {
            tracing::error!("Failed to get provider for workflow creation: {}", e);
            e
        })?;
        let model_config = provider.get_model_config();
        let model_name = &model_config.model_name;
        tracing::debug!("Using model: {}", model_name);

        let prompt_manager = self.prompt_manager.lock().await;
        let system_prompt = prompt_manager
            .builder()
            .with_extensions(extensions_info.into_iter())
            .with_frontend_instructions(self.frontend_instructions.lock().await.clone())
            .build();

        let workflow_prompt = prompt_manager.get_workflow_prompt().await;
        let tools = self
            .extension_manager
            .get_prefixed_tools(None)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get tools for workflow creation: {}", e);
                e
            })?;

        messages.push(Message::user().with_text(workflow_prompt));

        let (messages, issues) = fix_conversation(messages);
        if !issues.is_empty() {
            issues
                .iter()
                .for_each(|issue| tracing::warn!(workflow.conversation.issue = issue));
        }

        tracing::debug!(
            "Added workflow prompt to messages, total messages: {}",
            messages.len()
        );

        tracing::info!("Calling provider to generate workflow content");
        let (result, _usage) = self
            .provider
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| {
                let error = anyhow!("Provider not available during workflow creation");
                tracing::error!("{}", error);
                error
            })?
            .complete(&system_prompt, messages.messages(), &tools)
            .await
            .map_err(|e| {
                tracing::error!("Provider completion failed during workflow creation: {}", e);
                e
            })?;

        let content = result.as_concat_text();
        tracing::debug!(
            "Provider returned content with {} characters",
            content.len()
        );

        // the response may be contained in ```json ```, strip that before parsing json
        let re = Regex::new(r"(?s)```[^\n]*\n(.*?)\n```").unwrap();
        let clean_content = re
            .captures(&content)
            .and_then(|caps| caps.get(1).map(|m| m.as_str()))
            .unwrap_or(&content)
            .trim()
            .to_string();

        let json_content = serde_json::from_str::<Value>(&clean_content).ok();

        let (instructions, activities) = if let Some(json_content) = json_content.as_ref() {
            let instructions = json_content
                .get("instructions")
                .ok_or_else(|| anyhow!("Missing 'instructions' in json response"))?
                .as_str()
                .ok_or_else(|| anyhow!("instructions' is not a string"))?
                .to_string();

            let activities = json_content
                .get("activities")
                .ok_or_else(|| anyhow!("Missing 'activities' in json response"))?
                .as_array()
                .ok_or_else(|| anyhow!("'activities' is not an array'"))?
                .iter()
                .map(|act| {
                    act.as_str()
                        .map(|s| s.to_string())
                        .ok_or(anyhow!("'activities' array element is not a string"))
                })
                .collect::<Result<_, _>>()?;

            (instructions, activities)
        } else {
            tracing::warn!("Failed to parse JSON, falling back to string parsing");
            // If we can't get valid JSON, try string parsing
            // Use split_once to get the content after "Instructions:".
            let after_instructions = content
                .split_once("instructions:")
                .map(|(_, rest)| rest)
                .unwrap_or(&content);

            // Split once more to separate instructions from activities.
            let (instructions_part, activities_text) = after_instructions
                .split_once("activities:")
                .unwrap_or((after_instructions, ""));

            let instructions = instructions_part
                .trim_end_matches(|c: char| c.is_whitespace() || c == '#')
                .trim()
                .to_string();
            let activities_text = activities_text.trim();

            // Regex to remove bullet markers or numbers with an optional dot.
            let bullet_re = Regex::new(r"^[•\-*\d]+\.?\s*").expect("Invalid regex");

            // Process each line in the activities section.
            let activities: Vec<String> = activities_text
                .lines()
                .map(|line| bullet_re.replace(line, "").to_string())
                .map(|s| s.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect();

            (instructions, activities)
        };

        let extension_configs = self.get_extension_configs().await;

        let author = Author {
            contact: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .ok(),
            metadata: None,
        };

        // Ideally we'd get the name of the provider we are using from the provider itself,
        // but it doesn't know and the plumbing looks complicated.
        let config = Config::global();
        let provider_name: String = config
            .get_biorouter_provider()
            .expect("No provider configured. Run 'biorouter configure' first");

        let settings = Settings {
            biorouter_provider: Some(provider_name.clone()),
            biorouter_model: Some(model_name.clone()),
            temperature: Some(model_config.temperature.unwrap_or(0.0)),
        };

        tracing::debug!(
            "Building workflow with {} activities and {} extensions",
            activities.len(),
            extension_configs.len()
        );

        let (title, description) = if let Some(json_content) = json_content.as_ref() {
            let title = json_content
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("Custom workflow from chat")
                .to_string();

            let description = json_content
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("a custom workflow instance from this chat session")
                .to_string();

            (title, description)
        } else {
            (
                "Custom workflow from chat".to_string(),
                "a custom workflow instance from this chat session".to_string(),
            )
        };

        let skills = json_content
            .as_ref()
            .and_then(|json| json.get("skills"))
            .and_then(|skills| skills.as_array())
            .map(|skills| {
                skills
                    .iter()
                    .filter_map(|skill| skill.as_str())
                    .map(str::trim)
                    .filter(|skill| !skill.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut workflow_builder = Workflow::builder()
            .title(title)
            .description(description)
            .instructions(instructions)
            .activities(activities)
            .extensions(extension_configs)
            .settings(settings)
            .author(author);

        if !skills.is_empty() {
            workflow_builder = workflow_builder.skills(skills);
        }

        let workflow = workflow_builder.build().map_err(|e| {
            tracing::error!("Failed to build workflow: {}", e);
            anyhow!("Workflow build failed: {}", e)
        })?;

        tracing::info!("Workflow creation completed successfully");
        Ok(workflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{Permission, PermissionConfirmation};
    use crate::workflow::Response;

    #[test]
    fn compaction_notice_uses_user_facing_chat_terminology() {
        assert_eq!(
            COMPACTION_THINKING_TEXT,
            "biorouter is compacting the chat..."
        );
    }

    /// The merge that rebuilds a signed reply into one row must fire ONLY for a
    /// signed turn. It is named `merged_into_signed_turn` at the call site, and
    /// for a while that was the only thing making it signed-specific — the
    /// predicate was missing, so it ran for every provider.
    ///
    /// That is not cosmetic. `Conversation::push` merges only an *adjacent*
    /// same-id row; this one merges by id anywhere in the pending list. With
    /// tool-call batching off, an unsigned provider emits `[ToolRequest]` and
    /// then `[Text]` under one id, with the tool RESULT row appended between
    /// them — so an ungated merge folds the model's post-tool prose into the row
    /// *before* the result it describes, and the transcript reads backwards.
    #[test]
    fn the_signed_rebuild_merge_does_not_capture_unsigned_providers() {
        let signed = Message::assistant()
            .with_id("m1")
            .with_thinking("weighing it", "sig");
        let unsigned = Message::assistant()
            .with_id("m1")
            .with_thinking("weighing it", "");

        // An unsigned continuation of an unsigned row: must NOT merge.
        let pending = Conversation::new_unvalidated(vec![unsigned.clone()]);
        let continuation = Message::assistant().with_id("m1").with_text("the answer");
        assert!(!continues_signed_turn(&continuation, &pending));

        // The same continuation after a SIGNED row under that id: must merge,
        // because the signature covers the grouping and it has to be rebuilt.
        let pending = Conversation::new_unvalidated(vec![signed.clone()]);
        assert!(continues_signed_turn(&continuation, &pending));

        // A response that carries the signature itself qualifies on its own.
        let empty = Conversation::new_unvalidated(vec![]);
        assert!(continues_signed_turn(&signed, &empty));
        assert!(!continues_signed_turn(&unsigned, &empty));

        // Redacted thinking counts as signed even with no signature string.
        let redacted = Message::assistant()
            .with_id("m2")
            .with_redacted_thinking("bytes");
        assert!(continues_signed_turn(&redacted, &empty));
    }

    /// Extract the elicitation id from a queued request message.
    fn queued_elicitation_id(messages: &[Message]) -> Option<String> {
        use crate::conversation::message::ActionRequiredData;
        messages.iter().find_map(|msg| {
            msg.content.iter().find_map(|content| match content {
                MessageContent::ActionRequired(action) => match &action.data {
                    ActionRequiredData::Elicitation { id, .. } => Some(id.clone()),
                    _ => None,
                },
                _ => None,
            })
        })
    }

    #[tokio::test]
    async fn rewritten_tool_error_keeps_code_data_and_audit_through_post_hook_block() {
        let agent = Agent::new();
        let request_id = "rewritten-error".to_string();
        let response = Arc::new(Mutex::new(Message::user().with_id("response")));
        let response_map = HashMap::from([(request_id.clone(), response.clone())]);
        let original = CallToolRequestParams {
            task: None,
            meta: None,
            name: "developer__shell".into(),
            arguments: Some(object!({"command": "original"})),
        };
        let executed = CallToolRequestParams {
            task: None,
            meta: None,
            name: "developer__shell".into(),
            arguments: Some(object!({"command": "rewritten"})),
        };
        let mut install_ok = true;
        let mut post_results = Vec::new();
        agent
            .integrate_tool_result(
                request_id.clone(),
                Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "original failure",
                    None,
                )),
                &[],
                &response_map,
                &HashMap::from([(request_id.clone(), "developer__shell".to_string())]),
                &HashMap::from([(request_id.clone(), original)]),
                &HashMap::from([(request_id.clone(), executed)]),
                &HashMap::from([(request_id.clone(), None)]),
                &mut install_ok,
                &mut post_results,
                crate::guardrails::tool_output::ToolOutputGuardrailMode::Off,
                crate::agents::tool_errors::ToolErrorTaxonomyConfig::default(),
            )
            .await;
        agent
            .apply_post_tool_block(
                &request_id,
                "developer__shell",
                "blocked after execution",
                &response_map,
            )
            .await;

        let response = response.lock().await;
        let MessageContent::ToolResponse(tool_response) = &response.content[0] else {
            panic!("expected tool response");
        };
        let error = tool_response
            .tool_result
            .as_ref()
            .expect_err("post-hook block must preserve the Err variant");
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert!(error.message.contains("original failure"));
        assert!(error.message.contains("blocked after execution"));
        let data = error.data.as_ref().unwrap();
        assert_eq!(
            data[crate::agents::tool_errors::ENVELOPE_KEY]["kind"],
            "invalid_args"
        );
        assert_eq!(
            data["biorouterToolExecution"]["providerAuthored"]["arguments"]["command"],
            "original"
        );
        assert_eq!(
            data["biorouterToolExecution"]["actuallyExecuted"]["arguments"]["command"],
            "rewritten"
        );
    }

    /// #40: an elicitation request must preempt a tool batch whose only tool
    /// is parked on that very elicitation. `combined` here is the shape of
    /// such a batch — a stream that will never yield — so if the wake still
    /// depended on `combined.next()`, the 5 s timeout below would fire long
    /// before the production 300 s elicitation timeout resolved anything.
    /// The tail of the test asserts the full headless path: the queued
    /// request can be drained and cancelled, and the cancel unparks the
    /// waiting tool call promptly, without any other stream item arriving.
    #[tokio::test]
    async fn elicitation_preempts_a_parked_tool_batch() {
        use crate::action_required_manager::ActionRequiredManager;
        use std::time::Duration;

        const SESSION: &str = "preempt-test-session";
        let mut combined = futures::stream::pending::<(String, u8)>();

        let waiter = tokio::spawn(async {
            ActionRequiredManager::global()
                .request_and_wait(
                    "Need input".to_string(),
                    serde_json::json!({}),
                    Duration::from_secs(300),
                    Some(SESSION),
                )
                .await
        });

        let wake = tokio::time::timeout(
            Duration::from_secs(5),
            next_batch_wake(&None, &mut combined, SESSION),
        )
        .await
        .expect("the elicitation must preempt the parked batch");
        assert!(
            matches!(wake, BatchWake::ElicitationReady),
            "expected ElicitationReady, got {wake:?}"
        );

        // Drain the queued request exactly as drain_elicitation_messages
        // would, then cancel it the way a headless CLI run does.
        let drained = ActionRequiredManager::global().drain_requests(SESSION);
        let id = queued_elicitation_id(&drained)
            .expect("the request message carries the elicitation id");
        ActionRequiredManager::global()
            .submit_cancellation(id)
            .await
            .unwrap();

        let outcome = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("cancel must resolve without waiting for another stream item")
            .unwrap()
            .unwrap();
        assert_eq!(outcome, None, "headless cancel resolves as Ok(None)");
    }

    /// #40 round 3: the wake and the drain are scoped per session. With two
    /// concurrent daemon sessions, session B's elicitation must neither wake
    /// session A's parked batch loop nor be drained by it — before scoping,
    /// A's loop could win the process-global race and persist/yield B's
    /// prompt under A's session id, leaking it to the wrong UI.
    #[tokio::test]
    async fn a_sessions_loop_never_drains_another_sessions_elicitation() {
        use crate::action_required_manager::ActionRequiredManager;
        use std::time::Duration;

        const SESSION_A: &str = "two-session-test-a";
        const SESSION_B: &str = "two-session-test-b";
        let mut combined = futures::stream::pending::<(String, u8)>();

        let waiter = tokio::spawn(async {
            ActionRequiredManager::global()
                .request_and_wait(
                    "Need B's input".to_string(),
                    serde_json::json!({}),
                    Duration::from_secs(300),
                    Some(SESSION_B),
                )
                .await
        });

        // Synchronize on the request being queued via B's own wake seam.
        tokio::time::timeout(
            Duration::from_secs(5),
            ActionRequiredManager::global().request_arrived(SESSION_B),
        )
        .await
        .expect("the owning session must be woken");

        // Session A's batch loop: B's request must NOT preempt it — the only
        // way out of this race within the timeout would be the elicitation
        // wake, and it must stay parked.
        assert!(
            tokio::time::timeout(
                Duration::from_millis(300),
                next_batch_wake::<(String, u8), _>(&None, &mut combined, SESSION_A),
            )
            .await
            .is_err(),
            "session A's loop must not be woken by session B's elicitation"
        );

        // Even an unconditional drain by A (the post-item / post-batch drains
        // in the reply loop) must not surface B's request.
        let drained_by_a = ActionRequiredManager::global().drain_requests(SESSION_A);
        assert!(
            queued_elicitation_id(&drained_by_a).is_none(),
            "session A's drain must never return session B's request"
        );

        // The request is still intact for B's own loop, which can drain and
        // cancel it as usual.
        let drained_by_b = ActionRequiredManager::global().drain_requests(SESSION_B);
        let id = queued_elicitation_id(&drained_by_b)
            .expect("session B's request must still be deliverable to B");
        ActionRequiredManager::global()
            .submit_cancellation(id)
            .await
            .unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("cancel must unpark the waiter")
            .unwrap()
            .unwrap();
        assert_eq!(outcome, None);
    }

    fn confirmation(permission: Permission) -> PermissionConfirmation {
        PermissionConfirmation {
            principal_type: crate::permission::permission_confirmation::PrincipalType::Tool,
            permission,
        }
    }

    /// #41 regression: the exact shape the Bedrock decoder used to produce —
    /// two assistant tool-request messages sharing one id, separated by a
    /// user tool-response (so `Conversation::push` cannot merge them). Before
    /// the guard, persisting this batch aborted the turn with SQLite 2067
    /// (UNIQUE constraint failed: messages.session_id, messages.msg_uid).
    #[test]
    fn remint_gives_a_fresh_id_to_a_nonadjacent_duplicate() {
        let mut batch = Conversation::default();
        batch.push(
            Message::assistant()
                .with_id("shared-id".to_string())
                .with_tool_request(
                    "call_a",
                    Ok(rmcp::model::CallToolRequestParams {
                        task: None,
                        name: "shell".into(),
                        arguments: Some(rmcp::object!({"command": "ls"})),
                        meta: None,
                    }),
                ),
        );
        batch.push(
            Message::user()
                .with_id("resp-a".to_string())
                .with_text("tool response a"),
        );
        batch.push(
            Message::assistant()
                .with_id("shared-id".to_string())
                .with_tool_request(
                    "call_b",
                    Ok(rmcp::model::CallToolRequestParams {
                        task: None,
                        name: "shell".into(),
                        arguments: Some(rmcp::object!({"command": "pwd"})),
                        meta: None,
                    }),
                ),
        );
        // Not merged: the duplicate is non-adjacent.
        assert_eq!(batch.len(), 3);

        let fixed = remint_duplicate_message_ids(batch);
        let ids: Vec<String> = fixed
            .messages()
            .iter()
            .map(|m| m.id.clone().expect("all messages keep an id"))
            .collect();
        assert_eq!(ids.len(), 3);
        // The first occurrence keeps the provider's id (desktop delta-merge).
        assert_eq!(ids[0], "shared-id");
        assert_eq!(ids[1], "resp-a");
        // The later occurrence was re-minted, and every id is now unique.
        assert_ne!(ids[2], "shared-id");
        assert!(ids[2].starts_with("msg_"));
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), 3, "all ids must be distinct: {ids:?}");
        // Content is untouched.
        assert!(matches!(
            fixed.messages()[2].content[0],
            MessageContent::ToolRequest(_)
        ));
    }

    /// A batch with distinct (or absent) ids passes through byte-identical:
    /// the store mints a fresh uid per id-less message, so `None` ids are
    /// not duplicates of each other.
    #[test]
    fn remint_leaves_distinct_and_absent_ids_alone() {
        let mut batch = Conversation::default();
        batch.push(Message::assistant().with_id("a".to_string()).with_text("x"));
        batch.push(Message::user().with_text("no id 1"));
        batch.push(Message::assistant().with_id("b".to_string()).with_text("y"));
        batch.push(Message::user().with_text("no id 2"));

        let before: Vec<Option<String>> = batch.messages().iter().map(|m| m.id.clone()).collect();
        let fixed = remint_duplicate_message_ids(batch);
        let after: Vec<Option<String>> = fixed.messages().iter().map(|m| m.id.clone()).collect();
        assert_eq!(before, after, "no id may change when there is no duplicate");
    }

    /// BR-62's core safety property. Confirmations used to land on a single
    /// per-agent mpsc, so a decision for one request could be picked up by
    /// whatever tool call happened to be waiting — a late "allow" for a prompt
    /// the user had long since dismissed could approve an unrelated later call.
    /// Now each prompt owns its own channel, keyed by request id.
    #[tokio::test]
    async fn confirmation_reaches_only_its_own_request() {
        let agent = Agent::new();

        let rx_a = agent.register_confirmation("req-a");
        let rx_b = agent.register_confirmation("req-b");

        let outcome = agent
            .handle_confirmation("req-b".to_string(), confirmation(Permission::AllowOnce))
            .await;
        assert_eq!(outcome, ConfirmationOutcome::Delivered);

        // B got exactly the decision meant for it...
        let decided = rx_b.await.expect("b's prompt received its decision");
        assert_eq!(decided.permission, Permission::AllowOnce);

        // ...and A is untouched, still awaiting its own.
        assert!(agent.has_pending_confirmation("req-a"));
        assert!(!agent.has_pending_confirmation("req-b"));
        drop(rx_a);
    }

    /// A duplicate click, or a decision for a prompt that already expired or was
    /// cancelled, must be dropped — not applied to some other pending call. This
    /// is what makes `/action-required` safe to retry.
    #[tokio::test]
    async fn duplicate_and_stale_confirmations_are_dropped() {
        let agent = Agent::new();

        let rx = agent.register_confirmation("req-a");
        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AllowOnce))
                .await,
            ConfirmationOutcome::Delivered
        );
        let _ = rx.await;

        // Second click on the same card: nothing is waiting on that id any more.
        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AlwaysAllow))
                .await,
            ConfirmationOutcome::Unknown
        );

        // A decision for an id that was never registered at all.
        assert_eq!(
            agent
                .handle_confirmation(
                    "never-existed".to_string(),
                    confirmation(Permission::DenyOnce)
                )
                .await,
            ConfirmationOutcome::Unknown
        );
    }

    /// After a prompt is forgotten (it expired, or the turn was cancelled), a
    /// decision arriving late is reported as unknown rather than silently
    /// resolving anything.
    #[tokio::test]
    async fn a_forgotten_prompt_no_longer_accepts_a_decision() {
        let agent = Agent::new();

        let _rx = agent.register_confirmation("req-a");
        assert!(agent.has_pending_confirmation("req-a"));

        agent.forget_confirmation("req-a");
        assert!(!agent.has_pending_confirmation("req-a"));

        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AllowOnce))
                .await,
            ConfirmationOutcome::Unknown
        );

        // forget is idempotent.
        agent.forget_confirmation("req-a");
    }

    /// If the waiting side goes away (turn ended/cancelled) between the lookup and
    /// the send, the decision is dropped, not blamed on a live prompt.
    #[tokio::test]
    async fn a_decision_for_an_abandoned_prompt_is_unknown() {
        let agent = Agent::new();

        let rx = agent.register_confirmation("req-a");
        drop(rx);

        assert_eq!(
            agent
                .handle_confirmation("req-a".to_string(), confirmation(Permission::AllowOnce))
                .await,
            ConfirmationOutcome::Unknown
        );
    }

    #[tokio::test]
    async fn test_add_final_output_tool() -> Result<()> {
        let agent = Agent::new();

        let response = Response {
            json_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            })),
        };

        agent.add_final_output_tool(response).await;

        let tools = agent.list_tools("test-session-id", None).await;
        let final_output_tool = tools
            .iter()
            .find(|tool| tool.name == FINAL_OUTPUT_TOOL_NAME);

        assert!(
            final_output_tool.is_some(),
            "Final output tool should be present after adding"
        );

        let prompt_manager = agent.prompt_manager.lock().await;
        let system_prompt = prompt_manager.builder().build();

        let final_output_tool_ref = agent.final_output_tool.lock().await;
        let final_output_tool_system_prompt =
            final_output_tool_ref.as_ref().unwrap().system_prompt();
        assert!(system_prompt.contains(&final_output_tool_system_prompt));
        Ok(())
    }

    #[tokio::test]
    async fn apply_vault_resolves_secrets_in_tool_args() {
        use crate::agents::vault_refs::VaultRefs;
        use std::collections::HashMap;

        let agent = Agent::new();

        // No vault installed → arguments are untouched.
        let mut call = CallToolRequestParams {
            name: "files_read".into(),
            arguments: Some(
                serde_json::json!({ "header": "Bearer {{vault:API_KEY}}" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        };
        agent.apply_vault(&mut call).await;
        assert_eq!(
            call.arguments.as_ref().unwrap()["header"],
            serde_json::json!("Bearer {{vault:API_KEY}}"),
            "without a vault, placeholders are left intact"
        );

        // Install a vault → the placeholder resolves to the secret at dispatch.
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "sk-live-xyz".to_string());
        agent.set_vault(Arc::new(VaultRefs::new(secrets))).await;

        agent.apply_vault(&mut call).await;
        assert_eq!(
            call.arguments.as_ref().unwrap()["header"],
            serde_json::json!("Bearer sk-live-xyz"),
            "the installed vault resolves the secret into the args"
        );
    }

    #[tokio::test]
    async fn injected_skills_cache_is_per_session() {
        // BR-8: marking a skill injected is scoped to its session id, so a
        // different session (or a different skill) still gets the full body.
        let agent = Agent::new();

        assert!(!agent.skill_already_injected("s1", "demo").await);
        agent.mark_skill_injected("s1", "demo").await;

        assert!(agent.skill_already_injected("s1", "demo").await);
        // Same skill, different session → not yet injected.
        assert!(!agent.skill_already_injected("s2", "demo").await);
        // Different skill, same session → not yet injected.
        assert!(!agent.skill_already_injected("s1", "other").await);
    }

    #[tokio::test]
    async fn skill_resource_context_reinjects_on_load_failure() {
        // BR-8: a failed load must NOT be cached as "already injected" — the
        // next turn has to try again, and never silently emits the pointer in
        // place of a body that was never delivered.
        let agent = Agent::new();
        let refs = ResourceRefs {
            skills: vec!["nonexistent-skill".to_string()],
            ..Default::default()
        };

        let first = agent.skill_resource_context("sess", &refs).await;
        assert!(first.contains("Could not load this selected skill"));
        assert!(!first.contains("already loaded earlier in this session"));
        assert!(
            !agent
                .skill_already_injected("sess", "nonexistent-skill")
                .await
        );

        let second = agent.skill_resource_context("sess", &refs).await;
        assert!(second.contains("Could not load this selected skill"));
        assert!(!second.contains("already loaded earlier in this session"));
    }

    #[tokio::test]
    async fn skill_resource_context_pointer_after_injection() {
        // BR-8: once a skill is marked injected, later turns get the short
        // pointer instead of re-inlining the (potentially multi-KB) body.
        let agent = Agent::new();
        agent.mark_skill_injected("sess", "demo").await;

        let refs = ResourceRefs {
            skills: vec!["demo".to_string()],
            ..Default::default()
        };
        let out = agent.skill_resource_context("sess", &refs).await;
        assert!(out.contains("already loaded earlier in this session"));
        assert!(out.contains("skills__loadSkill"));
        assert!(!out.contains("Could not load this selected skill"));
    }

    #[tokio::test]
    async fn test_tool_inspection_manager_has_all_inspectors() -> Result<()> {
        let agent = Agent::new();

        // Verify that the tool inspection manager has all expected inspectors
        let inspector_names = agent.tool_inspection_manager.inspector_names();

        assert!(
            inspector_names.contains(&"repetition"),
            "Tool inspection manager should contain repetition inspector"
        );
        assert!(
            inspector_names.contains(&"permission"),
            "Tool inspection manager should contain permission inspector"
        );
        assert!(
            inspector_names.contains(&"security"),
            "Tool inspection manager should contain security inspector"
        );
        assert!(
            inspector_names.contains(&"managed"),
            "Tool inspection manager should contain managed policy inspector"
        );
        // #63: the consent gate for the machine-wide memory store. Its own tests
        // build a manager by hand, so *this* is the only thing that notices if
        // the agent stops registering it — at which point every global memory
        // becomes readable again with no prompt, and nothing else goes red.
        assert!(
            inspector_names.contains(&crate::security::global_memory::GLOBAL_MEMORY_INSPECTOR_NAME),
            "Tool inspection manager should contain the global-memory consent gate"
        );
        assert!(
            inspector_names.contains(&"sensitive_ops"),
            "Tool inspection manager should contain the sensitive-ops gate"
        );
        // BR-71 §5: the always-confirm hook for cross-session capability
        // changes. Its own unit tests all pass a `WorkspaceMutationInspector`
        // directly, so without this assertion deleting the registration would
        // leave every one of them green while the guarantee was gone.
        assert!(
            inspector_names.contains(&"workspace_mutation"),
            "Tool inspection manager should contain workspace mutation inspector"
        );

        Ok(())
    }

    /// BR-71 §5: registration ORDER, not merely presence.
    ///
    /// The confirmation card picks the text it shows with
    /// `inspection_results.iter().find(|r| r.tool_request_id == request.id)`
    /// and then reads the `RequireApproval(Some(msg))` payload off **that one
    /// result** (`agents/tool_execution.rs`, the `security_message` binding) —
    /// so the FIRST inspector to report on a request owns the explanation the
    /// user reads. `PermissionInspector` reports on *every* request it is given
    /// (`permission/permission_inspector.rs`, pass 1 pushes one result per
    /// request), and in Auto mode that result is a payload-less `Allow`.
    ///
    /// Registered after it, this inspector would still force the prompt — the
    /// merge in `apply_inspection_results_to_permissions` is escalation-only —
    /// but the "🔒 An agent is changing another conversation's capabilities"
    /// explanation would be silently dropped, leaving a bare, unexplained
    /// confirmation. That is the whole deliverable of §5 reduced to a shrug.
    ///
    /// Nothing else can catch it: every test in `agents::workspace_inspector`
    /// calls the inspector directly, and `security::sensitive_ops` never builds
    /// a `ToolInspectionManager` at all.
    #[tokio::test]
    async fn test_workspace_mutation_inspector_precedes_the_permission_inspector() {
        let agent = Agent::new();
        let names = agent.tool_inspection_manager.inspector_names();

        let workspace = names
            .iter()
            .position(|n| *n == "workspace_mutation")
            .expect("workspace mutation inspector must be registered");
        let permission = names
            .iter()
            .position(|n| *n == "permission")
            .expect("permission inspector must be registered");

        assert!(
            workspace < permission,
            "workspace_mutation must be registered BEFORE permission, or its \
             approval message loses the first-result-wins selection in \
             tool_execution.rs and the user sees an unexplained prompt; got {names:?}"
        );
    }

    /// BR-71: the soft-interrupt queue carries each injection's origin from the
    /// producer (a workspace steer, the subagent tab) through to the turn loop
    /// that drains it. Exercises the REAL queue on a real `Agent` — the same
    /// `drain_soft_interrupts` the reply loop consumes — not a stand-in `Mutex`.
    #[tokio::test]
    async fn soft_interrupt_queue_round_trips_provenance_through_the_real_agent() {
        use crate::conversation::message::{MessageProvenance, ProvenanceKind};

        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let agent = Agent::with_config(AgentConfig::new(
            sm,
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        ));

        // Legacy entry point still works and stamps nothing.
        agent.queue_soft_interrupt("plain".into());
        // Stamped entry point (BR-71): used by workspace steer + the subagent tab.
        agent.queue_soft_interrupt_with_provenance(
            "steer".into(),
            Some(MessageProvenance {
                kind: ProvenanceKind::AgentInjection,
                from_session_id: Some("s1".into()),
                from_session_name: None,
            }),
        );
        assert!(agent.has_soft_interrupts());

        // drain_soft_interrupts is exactly what the turn loop consumes.
        let drained = agent.drain_soft_interrupts();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].text, "plain");
        assert!(drained[0].provenance.is_none());
        assert_eq!(drained[1].text, "steer");
        assert!(matches!(
            drained[1].provenance.as_ref().unwrap().kind,
            ProvenanceKind::AgentInjection
        ));
        assert_eq!(
            drained[1]
                .provenance
                .as_ref()
                .unwrap()
                .from_session_id
                .as_deref(),
            Some("s1")
        );
        assert!(!agent.has_soft_interrupts(), "drain empties the queue");
    }

    /// BR-71: a poisoned soft-interrupt mutex must not silently swallow an
    /// injection.
    ///
    /// `queue_soft_interrupt_with_provenance` returns `()`, so a caller can
    /// never learn its text was dropped. That was survivable while the only
    /// producer was `POST /interrupt` — a lost keystroke the human is watching
    /// for and can retype. Once a *calling agent* gets a success back from
    /// `workspace_send_prompt mode:"steer"`, a dropped injection is an
    /// acknowledged-but-undelivered cross-session message with no observer at
    /// all. The queue holds a `Vec` and is only ever pushed to or taken from,
    /// so recovering the guard past a poison loses nothing.
    #[tokio::test]
    async fn a_poisoned_soft_interrupt_queue_still_accepts_and_drains() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let agent = Agent::with_config(AgentConfig::new(
            sm,
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        ));

        // Poison the queue's mutex the only way it can be poisoned: panic while
        // its guard is held.
        let queue = agent.soft_interrupts.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = queue.lock().unwrap();
            panic!("deliberately poisoning the soft-interrupt queue");
        }));
        assert!(
            agent.soft_interrupts.lock().is_err(),
            "precondition: the queue's mutex must actually be poisoned"
        );

        agent.queue_soft_interrupt("after the poison".into());
        assert!(
            agent.has_soft_interrupts(),
            "a poisoned lock must not swallow a queued injection"
        );

        let drained = agent.drain_soft_interrupts();
        assert_eq!(drained.len(), 1, "the injection must still be drainable");
        assert_eq!(drained[0].text, "after the poison");
        assert!(!agent.has_soft_interrupts(), "drain empties the queue");
    }

    /// A bare `Agent` on a throwaway session store — enough for the queue-level
    /// tests below, which never touch the provider or the history.
    async fn test_agent() -> Agent {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        // The store is never read here, but the dir must outlive the agent.
        std::mem::forget(temp);
        Agent::with_config(AgentConfig::new(
            sm,
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        ))
    }

    /// #69: an interrupt that arrives after the loop has committed to exiting must be
    /// REFUSED, not queued into whatever turn runs next.
    ///
    /// The old shape returned 202 unconditionally: the route observed an active turn,
    /// awaited the agent lookup, and pushed — while the loop performed its final
    /// empty-queue check and exited. The message then surfaced in an unrelated turn.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_interrupt_after_the_close_is_refused_not_deferred() {
        let agent = test_agent().await;

        agent.open_for_turn(TurnId::new("turn-a"));

        // The loop reaches its exit with nothing queued: close and drain in one step.
        assert!(matches!(agent.close_and_drain(), Drained::Empty));

        // A steer arrives one instant too late.
        let refused = agent.try_queue_soft_interrupt("too late".into(), None);
        assert!(
            matches!(refused, Err(InterruptRefused::TurnEnded)),
            "an interrupt after the close must be refused; got {refused:?}"
        );

        // And it must not be sitting in the queue waiting to ambush the next turn.
        agent.open_for_turn(TurnId::new("turn-b"));
        assert!(
            matches!(agent.close_and_drain(), Drained::Empty),
            "the refused interrupt must not have been queued for a later turn"
        );
    }

    /// The mirror: an interrupt that arrives while the turn is still accepting is
    /// taken, and is consumed by THAT turn rather than a later one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_interrupt_before_the_close_is_consumed_by_its_own_turn() {
        let agent = test_agent().await;
        agent.open_for_turn(TurnId::new("turn-a"));

        let landed = agent
            .try_queue_soft_interrupt("in time".into(), None)
            .expect("an open turn must accept");
        assert_eq!(
            landed.as_str(),
            "turn-a",
            "the caller is told which turn took it"
        );

        match agent.close_and_drain() {
            Drained::Some(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].text, "in time");
            }
            Drained::Empty => panic!("the queued steer must be drained by its own turn"),
        }
    }

    /// An `Agent` over a throwaway session store with exactly ONE loaded
    /// extension, in the requested mode.
    ///
    /// The single extension is not decoration: `subagents_enabled`'s final
    /// expression refuses when the extension list is empty, so an agent with no
    /// extensions can never satisfy the precondition these tests assert.
    async fn agent_for_tests(mode: crate::config::BioRouterMode) -> (Agent, String) {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let session = sm
            .create_session(
                temp.path().to_path_buf(),
                "tools".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        std::mem::forget(temp); // keep the sqlite file alive for the test
        let agent = Agent::with_config(AgentConfig::new(
            sm,
            crate::config::permission::PermissionManager::instance(),
            None,
            mode,
        ));
        // One loaded extension, so the "no extensions ⇒ no subagents" gate passes.
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "todo".into(),
                description: "todo".into(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .unwrap();
        (agent, session.id)
    }

    async fn agent_with_one_extension_for_tests() -> (Agent, String) {
        agent_for_tests(crate::config::BioRouterMode::Auto).await
    }

    async fn agent_in_chat_mode_for_tests() -> (Agent, String) {
        agent_for_tests(crate::config::BioRouterMode::Chat).await
    }

    /// Decision 21: a session with subagents enabled and NO explicit workspace
    /// entry still gets a spawn tool. This is the regression that would
    /// otherwise break every existing config when Task 19 lands.
    #[tokio::test]
    async fn subagents_enabled_injects_the_workspace_extension_with_the_spawn_tool_only() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        assert!(agent.subagents_enabled(&session_id).await, "precondition");

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names.iter().any(|n| n == "workspace__subagent"),
            "spawn tool must be advertised under the workspace prefix: {names:?}"
        );
        // …and none of the cross-session surface came with it.
        assert!(
            !names.iter().any(|n| n.starts_with("workspace__workspace_")),
            "auto-injection must not grant cross-session control: {names:?}"
        );

        // THE 18 → 19 BOUNDARY, now crossed. Task 18 deliberately advertised
        // BOTH the bare `subagent` (the standalone push, the only *callable*
        // path while dispatch still matched on the bare name) and the prefixed
        // `workspace__subagent`, for exactly one commit. Task 19 deleted the
        // standalone push and taught dispatch both name forms
        // (`is_spawn_tool_call`), so the duplicate advertisement is gone: the
        // extension is the ONE place the spawn tool is advertised (decision 20).
        // The bare name stays *dispatchable* for prefix-stripping models — it is
        // simply no longer *listed* twice.
        assert!(
            !names.iter().any(|n| n == SUBAGENT_TOOL_NAME),
            "after Task 19 the workspace extension is the only advertisement; \
             the standalone bare `subagent` must be gone: {names:?}"
        );
    }

    /// BR-71 decision 23: `subagent_status` is REMOVED, not renamed. Its three
    /// jobs are workspace tools now (list → `workspace_list`, poll →
    /// `workspace_read_conversation`, wait → `workspace_watch`, cancel →
    /// `workspace_close`), all of which also work for foreground children and
    /// for the human.
    ///
    /// The env guard is load-bearing. The tool was only ever offered when
    /// `subagent_handle::background_enabled()` is true, and that reads
    /// `BIOROUTER_SUBAGENT_BACKGROUND`, which defaults to FALSE. Without
    /// opening the gate this test is green before the deletion too, and a
    /// botched deletion would still show green.
    #[tokio::test]
    async fn no_session_advertises_subagent_status_any_more() {
        let _guard = env_lock::lock_env([("BIOROUTER_SUBAGENT_BACKGROUND", Some("true"))]);

        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        assert!(
            crate::agents::subagent_handle::background_enabled(),
            "precondition: the gate that used to offer the tool is OPEN"
        );

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains("subagent_status")),
            "decision 23: the tool is removed, not renamed: {names:?}"
        );
        assert!(
            names.iter().any(|n| n == "workspace__subagent"),
            "…and delegation itself still works: {names:?}"
        );
    }

    /// Both spellings reach the spawn interception. A model that strips
    /// extension prefixes calls `subagent`; everything else calls
    /// `workspace__subagent`. Neither may fall through to the extension
    /// manager, which would land on the extension's "dispatched by the agent
    /// loop" error arm.
    #[test]
    fn dispatch_recognizes_both_spawn_tool_name_forms() {
        assert!(is_spawn_tool_call("workspace__subagent"));
        assert!(is_spawn_tool_call("subagent"));
        assert!(!is_spawn_tool_call("workspace__workspace_list"));
        assert!(!is_spawn_tool_call("subagent_status")); // never a spawn call
    }

    #[test]
    fn subagent_sessions_are_refused_workspace_tools() {
        use crate::session::session_manager::SessionType;
        // Decision 25 + §5: no delegation-tree fan-out of workspace control,
        // and no child steering its parent.
        for tool in [
            "workspace__workspace_list",
            "workspace_list",
            "workspace__workspace_send_prompt",
            "workspace__subagent",
            "subagent",
        ] {
            assert!(
                is_workspace_tool_refused_for(SessionType::SubAgent, tool),
                "{tool} must be refused inside a subagent"
            );
        }
        assert!(!is_workspace_tool_refused_for(
            SessionType::User,
            "workspace_list"
        ));
        assert!(!is_workspace_tool_refused_for(
            SessionType::SubAgent,
            "developer__shell"
        ));
    }

    #[test]
    fn the_workspace_guard_does_not_swallow_a_third_party_extension() {
        use crate::session::session_manager::SessionType;
        // A third-party extension NAMED `workspace_foo` advertises its tools as
        // `workspace_foo__bar`, which starts with "workspace_". It has nothing
        // to do with BR-71 and must run inside a subagent like any other tool.
        assert!(!is_workspace_tool_refused_for(
            SessionType::SubAgent,
            "workspace_foo__bar"
        ));
        assert!(!is_workspace_tool_refused_for(
            SessionType::SubAgent,
            "workspace_analytics__query"
        ));
        // …while every real workspace tool, in both spellings, still is.
        for name in WORKSPACE_TOOL_NAMES {
            assert!(is_workspace_tool_refused_for(SessionType::SubAgent, name));
            assert!(is_workspace_tool_refused_for(
                SessionType::SubAgent,
                &format!("workspace__{name}")
            ));
        }
    }

    /// The rot vector both the guard's own comment and its over-match test
    /// share: `WORKSPACE_TOOL_NAMES` is a hand-maintained mirror of what the
    /// workspace extension advertises, and
    /// `the_workspace_guard_does_not_swallow_a_third_party_extension` iterates
    /// that same list — so an eighth `workspace_*` tool added to `get_tools()`
    /// later would be dispatchable inside a delegation tree with nothing
    /// failing. Cross-check against the real advertisement, both directions:
    /// a new tool that is not refused, and a refused name that no longer
    /// exists, are both errors.
    #[test]
    fn the_refusal_list_mirrors_every_tool_the_workspace_extension_advertises() {
        use crate::session::session_manager::SessionType;
        let all: Vec<String> = crate::agents::workspace_extension::WorkspaceClient::get_tools()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();

        // The spawn tool rides in `get_tools()` too (decision 22 moved it into
        // this extension), but it is named `subagent`, not `workspace_*`, and
        // `is_spawn_tool_call` — not this list — is what refuses it. Assert that
        // rather than assume it, then take it out of the mirror comparison.
        assert!(
            all.iter().any(|a| a == SUBAGENT_TOOL_NAME),
            "the spawn tool is advertised by the workspace extension: {all:?}"
        );
        assert!(is_workspace_tool_refused_for(
            SessionType::SubAgent,
            SUBAGENT_TOOL_NAME
        ));
        let advertised: Vec<String> = all
            .into_iter()
            .filter(|n| n != SUBAGENT_TOOL_NAME)
            .collect();

        for name in &advertised {
            assert!(
                WORKSPACE_TOOL_NAMES.contains(&name.as_str()),
                "{name} is advertised by the workspace extension but is not in \
                 WORKSPACE_TOOL_NAMES, so a subagent could call it"
            );
        }
        for name in WORKSPACE_TOOL_NAMES {
            assert!(
                advertised.iter().any(|a| a == name),
                "{name} is refused for subagents but is no longer advertised, so \
                 the list is stale"
            );
        }
        // …and the spawn tool stays out of the name list itself, so the two
        // mechanisms cannot both claim it and the refusal message stays
        // specific ("cannot create other subagents", not "workspace tools").
        assert!(!WORKSPACE_TOOL_NAMES.contains(&SUBAGENT_TOOL_NAME));
    }

    /// The CALL SITE, not the predicate. `is_workspace_tool_refused_for` is a
    /// pure function; deleting its one invocation at the top of
    /// `dispatch_tool_call` leaves every other test in this file green while a
    /// subagent regains the entire workspace surface. This drives the real
    /// dispatcher with a real `SessionType::SubAgent` session and pins both
    /// branches of the two-message refusal.
    #[tokio::test]
    async fn dispatch_refuses_workspace_tools_and_nesting_inside_a_subagent() {
        use crate::session::session_manager::SessionType;

        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let child = sm
            .create_session(
                temp.path().to_path_buf(),
                "child".into(),
                SessionType::SubAgent,
            )
            .await
            .unwrap();
        let parent = sm
            .create_session(
                temp.path().to_path_buf(),
                "parent".into(),
                SessionType::User,
            )
            .await
            .unwrap();
        std::mem::forget(temp);
        let agent = Agent::with_config(AgentConfig::new(
            sm,
            crate::config::permission::PermissionManager::instance(),
            None,
            crate::config::BioRouterMode::Auto,
        ));

        let call = |name: &str| CallToolRequestParams {
            meta: None,
            name: name.to_string().into(),
            arguments: Some(serde_json::Map::new()),
            task: None,
        };
        let refusal = |result: Result<ToolCallResult, ErrorData>| match result {
            // `ToolCallResult` is not `Debug`, so match rather than unwrap_err.
            Ok(_) => panic!("the guard must refuse before anything dispatches"),
            Err(e) => e,
        };

        // Cross-session control, prefixed and bare.
        for name in ["workspace__workspace_send_prompt", "workspace_close"] {
            let (_id, result) = agent
                .dispatch_tool_call(call(name), "req".into(), None, &child)
                .await;
            let err = refusal(result);
            assert_eq!(err.code, ErrorCode::INVALID_REQUEST, "{name}");
            assert!(
                err.message.contains("cannot use workspace tools"),
                "{name}: {}",
                err.message
            );
        }

        // Nesting gets its OWN message, so the model learns the actual rule
        // rather than a generic "workspace tools" it did not think it used.
        for name in ["subagent", "workspace__subagent"] {
            let (_id, result) = agent
                .dispatch_tool_call(call(name), "req".into(), None, &child)
                .await;
            let err = refusal(result);
            assert_eq!(err.code, ErrorCode::INVALID_REQUEST, "{name}");
            assert!(
                err.message.contains("cannot create other subagents"),
                "{name}: {}",
                err.message
            );
        }

        // …and a USER session is untouched by the guard. It still fails (no
        // such extension is loaded), but not with the subagent refusal — which
        // is what a blanket "refuse workspace tools" implementation would give.
        let (_id, result) = agent
            .dispatch_tool_call(
                call("workspace__workspace_list"),
                "req".into(),
                None,
                &parent,
            )
            .await;
        if let Err(e) = result {
            assert!(
                !e.message.contains("Subagents cannot"),
                "a user session must not hit the subagent guard: {}",
                e.message
            );
        }
    }

    #[test]
    fn parking_workspace_tools_are_exempt_from_the_dispatch_semaphore() {
        for name in [
            "workspace_watch",
            "workspace__workspace_watch",
            "workspace_send_prompt",
            "workspace__workspace_send_prompt",
        ] {
            assert!(
                is_parking_workspace_tool(name),
                "{name} parks on another session and must not hold a permit"
            );
        }
        // Non-parking workspace tools stay bounded — they do their own work.
        for name in ["workspace_list", "workspace__workspace_set_tools"] {
            assert!(!is_parking_workspace_tool(name));
        }
    }

    /// The sub-workflow-enriched description survives the move. The extension
    /// advertises with `&[]` (it has no access to the agent's `sub_workflows`
    /// map), so `list_tools` must restore the enriched text — otherwise a
    /// session that defines sub-workflows silently stops telling the model they
    /// exist, which is invisible until someone notices the model never uses one.
    #[tokio::test]
    async fn sub_workflow_names_still_reach_the_spawn_tool_description() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        agent
            .add_sub_workflows(vec![crate::workflow::SubWorkflow {
                name: "test_workflow".to_string(),
                path: "test.yaml".to_string(),
                values: None,
                sequential_when_repeated: false,
                description: Some("A test workflow".to_string()),
            }])
            .await;

        let tools = agent.list_tools(&session_id, None).await;
        let spawn = tools
            .iter()
            .find(|t| t.name == "workspace__subagent")
            .expect("the spawn tool is advertised");
        let description = spawn.description.as_ref().unwrap();
        assert!(
            description.contains("Available subworkflows"),
            "got: {description}"
        );
        assert!(description.contains("test_workflow"), "got: {description}");
    }

    /// F-class regression: a restricted grant must bind the bare name too.
    #[tokio::test]
    async fn a_restricted_workspace_grant_refuses_the_bare_spawn_name() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                // Read-only grant: no spawning from this session.
                available_tools: vec!["workspace_list".to_string()],
            })
            .await
            .unwrap();
        assert!(
            !agent
                .extension_manager
                .is_extension_tool_available("workspace", "subagent")
                .await
        );
        let _ = session_id;
    }

    /// The dispatch half of the same guarantee: `available_tools` is enforced
    /// on the call path too (`extension_manager.rs`, the
    /// `config.is_tool_available` re-check in `dispatch_tool_call`), so a
    /// remembered tool name cannot reach the handler.
    #[tokio::test]
    async fn an_auto_injected_session_cannot_dispatch_a_cross_session_tool() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // triggers the injection
        let dispatched = agent
            .extension_manager
            .dispatch_tool_call(
                &session_id,
                rmcp::model::CallToolRequestParams {
                    meta: None,
                    name: "workspace__workspace_send_prompt".into(),
                    arguments: Some(
                        serde_json::json!({ "session_id": "other", "text": "hi", "mode": "note" })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                    task: None,
                },
                crate::privacy::CallCapability::for_test_restricted(),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        // `ToolCallResult` is not `Debug`, so `unwrap_err()` (which formats the
        // Ok payload) does not compile — match instead.
        let err = match dispatched {
            Ok(_) => panic!(
                "workspace_send_prompt is outside the auto-injection's \
                 available_tools and must not reach a handler"
            ),
            Err(e) => e,
        };
        assert!(err.to_string().contains("not available"), "got: {err}");
    }

    /// A user-enabled workspace entry keeps the full surface — the injection
    /// must never downgrade it.
    #[tokio::test]
    async fn an_explicit_workspace_entry_keeps_every_tool() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![], // empty = all
            })
            .await
            .unwrap();

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "{names:?}"
        );
        assert!(
            names.iter().any(|n| n == "workspace__subagent"),
            "{names:?}"
        );
    }

    /// The other order, and the one the plan's two-tier table is really about:
    /// the user turns Workspace Control on **after** a turn has already
    /// auto-injected the spawn-only entry.
    ///
    /// `ExtensionManager::add_extension` returns `Ok(())` without touching
    /// anything when the key is already loaded, so the explicit enable is a
    /// silent no-op unless `Agent::add_extension` evicts the injection first.
    /// The failure is doubly bad: the model keeps only `subagent`, and the
    /// spawn-only config is then persisted into the session row as the user's
    /// own decision, so the downgrade survives every reload.
    #[tokio::test]
    async fn an_explicit_enable_upgrades_an_auto_injected_entry() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;

        // A turn happens first: the spawn-only surface is injected.
        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            !names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "precondition: only the spawn tool was injected: {names:?}"
        );

        // Now the user enables Workspace Control in Settings.
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![], // empty = all
            })
            .await
            .unwrap();

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "an explicit enable must REPLACE the auto-injection, not be \
             swallowed by it: {names:?}"
        );
        assert!(
            names.iter().any(|n| n == "workspace__subagent"),
            "{names:?}"
        );

        // …and what persists is the user's full-surface config, not ours.
        let persisted: Vec<Vec<String>> = agent
            .persistable_extension_configs()
            .await
            .into_iter()
            .filter(|c| c.name() == "workspace")
            .map(|c| match c {
                crate::agents::extension::ExtensionConfig::Platform {
                    available_tools, ..
                } => available_tools,
                other => panic!("unexpected variant: {other:?}"),
            })
            .collect();
        assert_eq!(
            persisted,
            vec![Vec::<String>::new()],
            "the persisted entry must be the user's (empty = all), not the \
             injection's [\"subagent\"]"
        );
    }

    /// The session-load mirror of the same guarantee.
    ///
    /// `load_extensions_from_session` skips any name already loaded, so an
    /// injection that landed first (a turn ran before the load, or concurrently
    /// with it) would permanently shadow the session's OWN explicitly
    /// configured `workspace` entry with the spawn-only one — and, once
    /// `add_extension` clears the mark, persist that downgrade back to the row.
    #[tokio::test]
    async fn a_session_load_replaces_an_auto_injected_entry_it_would_otherwise_skip() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // injects spawn-only

        // The session's own configuration names workspace with the full surface.
        let state = crate::session::EnabledExtensionsState::new(vec![
            crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![],
            },
        ]);
        let mut session = agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
            .unwrap();
        state
            .to_extension_data(&mut session.extension_data)
            .unwrap();

        let agent = std::sync::Arc::new(agent);
        let results = agent.load_extensions_from_session(&session).await;
        assert!(
            results.iter().all(|r| r.success),
            "load must succeed: {results:?}"
        );

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "the session's own entry must win over the injection: {names:?}"
        );
    }

    /// The auto-injection must never reach the SESSION ROW. `persist_extension_state`
    /// snapshots every loaded extension, so without the exclusion this test's
    /// second half fails and Settings shows Workspace Control enabled on a
    /// session the user never touched.
    #[tokio::test]
    async fn an_auto_injected_extension_is_never_persisted_to_the_session() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // triggers the injection
        assert!(
            agent
                .extension_manager
                .is_extension_enabled("workspace")
                .await,
            "precondition: the injection happened"
        );

        // BOTH persist paths must filter, so assert on the SHARED helper first —
        // that is the one thing covering `save_extension_state` too. That method
        // is the reply loop's own path (fired whenever the model enables an
        // extension mid-turn through `manage_extensions`); it snapshots the same
        // set and, without the shared helper, is unfiltered.
        let persistable: Vec<String> = agent
            .persistable_extension_configs()
            .await
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert!(
            !persistable.contains(&"workspace".to_string()),
            "the filter both persist paths share must exclude the injection: {persistable:?}"
        );

        // The GUI toggling ANY extension, and workspace_set_tools, both land here.
        agent.persist_extension_state(&session_id).await.unwrap();

        let session = agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
            .unwrap();
        let persisted =
            crate::session::EnabledExtensionsState::from_extension_data(&session.extension_data)
                .expect("a state was written");
        assert!(
            !persisted.extensions.iter().any(|e| e.name() == "workspace"),
            "the auto-injection must not be recorded as a user decision: {:?}",
            persisted
                .extensions
                .iter()
                .map(|e| e.name().to_string())
                .collect::<Vec<_>>()
        );

        // …but an EXPLICIT add of the same extension does persist.
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .unwrap();
        agent.persist_extension_state(&session_id).await.unwrap();
        let session = agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
            .unwrap();
        let persisted =
            crate::session::EnabledExtensionsState::from_extension_data(&session.extension_data)
                .expect("a state was written");
        assert!(
            persisted.extensions.iter().any(|e| e.name() == "workspace"),
            "an explicit enable is a user decision and must be recorded"
        );
    }

    /// The SECOND persist path, driven end-to-end rather than through the shared
    /// helper.
    ///
    /// `save_extension_state` is the reply loop's own path — it fires on any
    /// turn where the model successfully enables an extension through
    /// `manage_extensions`, i.e. on exactly the population that gets the
    /// auto-injection (Auto mode, at least one extension). Asserting only on
    /// `persistable_extension_configs` would leave that path free to regress to
    /// its old unfiltered `get_extension_configs()` snapshot with every other
    /// test still green, so this one goes through the method itself and reads
    /// the SESSION ROW back.
    #[tokio::test]
    async fn the_reply_loop_save_path_also_excludes_the_auto_injection() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // triggers the injection
        assert!(
            agent
                .extension_manager
                .is_extension_enabled("workspace")
                .await,
            "precondition: the injection happened"
        );

        agent
            .save_extension_state(&SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: None,
                max_tool_calls: None,
                budget: None,
                retry_config: None,
                reasoning_effort: None,
            })
            .await
            .unwrap();

        let session = agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
            .unwrap();
        let persisted =
            crate::session::EnabledExtensionsState::from_extension_data(&session.extension_data)
                .expect("a state was written");
        assert!(
            !persisted.extensions.iter().any(|e| e.name() == "workspace"),
            "the reply loop's own persist path must exclude the injection too: {:?}",
            persisted
                .extensions
                .iter()
                .map(|e| e.name().to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            persisted.extensions.iter().any(|e| e.name() == "todo"),
            "…while still recording the extensions the user really has: {:?}",
            persisted
                .extensions
                .iter()
                .map(|e| e.name().to_string())
                .collect::<Vec<_>>()
        );
    }

    /// The MODEL's path to enabling an extension, which is not the user's.
    ///
    /// `manage_extensions` calls `ExtensionManager::add_extension` directly
    /// (see `extension_manager_extension.rs`) and never touches
    /// `Agent::add_extension`, so any "an explicit enable replaces the
    /// injection" logic that lives on the `Agent` is simply not on this path.
    /// The manager treats an already-loaded key as a no-op, so the model is
    /// told "installed successfully" while the spawn-only surface, and the
    /// injection's exemption from persistence, both survive untouched — a
    /// permanent silent no-op for `manage_extensions enable workspace`.
    #[tokio::test]
    async fn a_model_driven_enable_upgrades_an_auto_injected_entry() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // injects spawn-only

        // Byte-for-byte what `manage_extensions` does with the registry entry.
        agent
            .extension_manager
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .unwrap();

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "the model's enable must replace the injection, not be swallowed \
             by it: {names:?}"
        );

        // …and the reply loop's save, which fires on that very turn, must now
        // record it as the user-visible decision it has become.
        let persistable: Vec<String> = agent
            .persistable_extension_configs()
            .await
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert!(
            persistable.contains(&"workspace".to_string()),
            "an enable that really happened must persist: {persistable:?}"
        );
    }

    /// The reverse order, and the half no Agent-level bookkeeping can get
    /// right: the injection arrives when an explicit entry is ALREADY loaded.
    ///
    /// This is the tail of the real race — `ensure_spawn_extension` finds the
    /// key absent, an explicit enable lands, and only then does the injection's
    /// own add run. The manager answers `Ok(())` for the already-loaded key, so
    /// provenance recorded by the CALLER after that `Ok(())` marks the *user's*
    /// full-surface entry as auto-injected, and it silently stops persisting.
    /// Deciding provenance inside the same lock as the insert is what makes the
    /// interleaving unrepresentable; this pins the sequential shadow of it.
    #[tokio::test]
    async fn an_auto_injection_never_claims_an_existing_explicit_entry() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .unwrap();

        // The late half of the racing injection.
        agent
            .extension_manager
            .add_extension_auto_injected(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Delegate work to subagents".into(),
                bundled: Some(true),
                available_tools: vec![SUBAGENT_TOOL_NAME.to_string()],
            })
            .await
            .unwrap();

        let persistable: Vec<String> = agent
            .persistable_extension_configs()
            .await
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert!(
            persistable.contains(&"workspace".to_string()),
            "an injection must never claim provenance for an entry the user \
             enabled: {persistable:?}"
        );

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "…nor downgrade its surface: {names:?}"
        );
    }

    /// The session row is not the only place a derived grant can escape into
    /// something durable. `Agent::get_extension_configs` is the snapshot handed
    /// to a child agent's `TaskConfig` at the `subagent` dispatch, and the same
    /// snapshot is written into a generated workflow's `extensions` list — a
    /// file that outlives the session and is re-run later, on a machine where
    /// nothing re-derives `subagents_enabled`.
    #[tokio::test]
    async fn the_auto_injection_does_not_propagate_to_child_agents_or_workflows() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // triggers the injection
        assert!(
            agent
                .extension_manager
                .is_extension_enabled("workspace")
                .await,
            "precondition: the injection happened"
        );

        let inherited: Vec<String> = agent
            .get_extension_configs()
            .await
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        assert!(
            !inherited.contains(&"workspace".to_string()),
            "a derived per-turn grant must not be captured into a child agent \
             or a workflow file: {inherited:?}"
        );
        assert!(
            inherited.contains(&"todo".to_string()),
            "…while the extensions the user really has still propagate: \
             {inherited:?}"
        );
    }

    /// The injection is DERIVED state, so it has to go when its cause goes.
    ///
    /// Skipping the injection on a later turn is not enough: the extension is
    /// already loaded, and `get_prefixed_tools` reads the manager
    /// unconditionally. Without a revocation the grant is permanent — an
    /// Agent-Drafter app that turns delegation off (so `consult` is the one
    /// delegation mechanism, the whole point of `set_subagent_tool_enabled`),
    /// a switch to a Gemini model, or a mode change to Chat all leave
    /// `workspace__subagent` advertised, and the dispatch gate keys on
    /// `session_type`, not on `subagents_enabled`.
    #[tokio::test]
    async fn the_injection_is_revoked_when_delegation_stops_being_enabled() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names.iter().any(|n| n == "workspace__subagent"),
            "precondition: the injection happened: {names:?}"
        );

        // An app with declared worker profiles takes the generic spawn tool away.
        agent.set_subagent_tool_enabled(false);
        assert!(
            !agent.subagents_enabled(&session_id).await,
            "precondition: delegation is off"
        );

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains("subagent")),
            "no spawn tool may survive its own precondition: {names:?}"
        );
        assert!(
            !agent
                .extension_manager
                .is_extension_enabled("workspace")
                .await,
            "…and the derived grant itself is gone, not merely unadvertised"
        );
    }

    /// The gate must not hold itself open.
    ///
    /// `subagents_enabled` refuses when no extension is loaded — and the
    /// extension it causes to be loaded is an extension. Counted naively, one
    /// turn's injection satisfies the precondition for every later turn, so a
    /// session that removed its last real extension would keep delegating
    /// forever off the back of a grant it derived from itself.
    #[tokio::test]
    async fn the_injection_does_not_keep_the_subagent_gate_open_by_itself() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&session_id, None).await; // injects

        agent.remove_extension("todo").await.unwrap(); // the last real one
        assert!(
            !agent.subagents_enabled(&session_id).await,
            "an agent whose only remaining extension is its own injection has \
             no extensions in the sense the gate means"
        );

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(!names.iter().any(|n| n.contains("subagent")), "{names:?}");
    }

    /// ⚠ **The same rule, arriving by the other door** (#76).
    ///
    /// The test above covers an AUTO-INJECTED workspace. Once Workspace became
    /// a default-on capability it loads as `Explicit` instead, and an
    /// origin-only predicate would have been satisfied by workspace itself in
    /// every session, permanently — the gate would still compile, still pass
    /// that test, and mean nothing.
    ///
    /// So `has_non_injected_extensions` excludes it by NAME as well as by
    /// origin, and this is the test that says so. Without it the inversion of
    /// `default_enabled` is a silent semantic regression rather than a product
    /// decision.
    #[tokio::test]
    async fn an_explicit_workspace_does_not_keep_the_subagent_gate_open_either() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;

        // Explicit, exactly as a default-on capability arrives — not the
        // injection path.
        agent
            .add_extension(ExtensionConfig::Platform {
                name: Agent::SPAWN_EXTENSION.to_string(),
                description: "workspace".to_string(),
                bundled: Some(true),
                available_tools: Vec::new(),
            })
            .await
            .unwrap();

        agent.remove_extension("todo").await.unwrap(); // the last real one

        assert!(
            !agent.subagents_enabled(&session_id).await,
            "an explicitly-enabled workspace is still not a capability the USER              granted for the purpose the gate asks about"
        );
    }

    /// ACP loads the user's extensions onto ONE `Agent` and serves every
    /// session from it, so a grant recorded per extension name rather than per
    /// session is visible to sessions that were never eligible for it. The
    /// session type is the only axis of `subagents_enabled` that varies between
    /// two sessions of one agent, and it is the axis that says "subagents
    /// cannot create other subagents".
    #[tokio::test]
    async fn an_ineligible_session_on_a_shared_agent_is_not_offered_the_injection() {
        let (agent, eligible) = agent_with_one_extension_for_tests().await;
        let _ = agent.list_tools(&eligible, None).await; // the eligible session injects

        let working_dir = agent
            .config
            .session_manager
            .get_session(&eligible, false)
            .await
            .unwrap()
            .working_dir;
        let other = agent
            .config
            .session_manager
            .create_session(
                working_dir,
                "child".into(),
                crate::session::session_manager::SessionType::SubAgent,
            )
            .await
            .unwrap()
            .id;
        assert!(
            !agent.subagents_enabled(&other).await,
            "precondition: a subagent session may not delegate"
        );

        let names: Vec<String> = agent
            .list_tools(&other, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains("subagent")),
            "a grant another session derived must not be offered here: {names:?}"
        );
    }

    /// Revoking the injection is necessary but not sufficient: `subagent` is
    /// one of the workspace extension's own tools now, so a user who enabled
    /// Workspace Control explicitly is advertised the spawn tool whatever the
    /// gate says. The rule has to be about the TOOL, not about how the
    /// extension got loaded — no spawn tool, by any name, from any source,
    /// when delegation is off.
    #[tokio::test]
    async fn an_explicit_workspace_entry_still_hides_the_spawn_tool_when_delegation_is_off() {
        let (agent, session_id) = agent_in_chat_mode_for_tests().await;
        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Platform {
                name: "workspace".into(),
                description: "Workspace Control".into(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .unwrap();
        assert!(!agent.subagents_enabled(&session_id).await, "precondition");

        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n == "workspace__workspace_send_prompt"),
            "the user's own entry keeps its cross-session surface: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.contains("subagent")),
            "…but not a spawn tool the gate says this session may not have: \
             {names:?}"
        );
    }

    /// `extension_data` is ONE json column shared by every per-session
    /// extension — `enabled_extensions.v0` next to `todo.v1`, `goal.v0`,
    /// `run_state.*`, `workspace_skills.v1`. Both persist paths used to read
    /// the whole object, mutate their key in a local copy, and write the whole
    /// object back as two separate statements, so a writer of a DIFFERENT key
    /// that committed in between was silently erased by the later whole-column
    /// write. `SessionManager::update_extension_state` exists precisely for
    /// this and documents the hazard; these two paths were not using it.
    ///
    /// Tool calls overlap by construction (the loop drives them through
    /// `select_all`) and both of these paths are tool-triggered — the GUI
    /// extension toggle and `workspace_set_tools` on one, the reply loop's
    /// `manage_extensions` save on the other — so this is reachable, not
    /// theoretical.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn persisting_extension_state_does_not_erase_another_key_of_the_column() {
        let (agent, session_id) = agent_with_one_extension_for_tests().await;
        let agent = std::sync::Arc::new(agent);

        let session_manager = agent.config.session_manager.clone();
        let todo_session = session_id.clone();
        let todos = tokio::spawn(async move {
            for i in 0..40 {
                session_manager
                    .update_extension_state(
                        &todo_session,
                        crate::session::extension_data::TodoState::EXTENSION_NAME,
                        crate::session::extension_data::TodoState::VERSION,
                        move |_| Ok(serde_json::json!({ "items": [], "plan": format!("p{i}") })),
                    )
                    .await
                    .unwrap();
            }
        });

        let extension_agent = agent.clone();
        let extension_session = session_id.clone();
        let extensions = tokio::spawn(async move {
            for _ in 0..40 {
                extension_agent
                    .persist_extension_state(&extension_session)
                    .await
                    .unwrap();
            }
        });

        todos.await.unwrap();
        extensions.await.unwrap();

        let session = agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
            .unwrap();
        assert!(
            crate::session::EnabledExtensionsState::from_extension_data(&session.extension_data)
                .is_some(),
            "the extension state this test drives must be there"
        );
        assert!(
            session
                .extension_data
                .get_extension_state(
                    crate::session::extension_data::TodoState::EXTENSION_NAME,
                    crate::session::extension_data::TodoState::VERSION
                )
                .is_some(),
            "…and so must the unrelated key a concurrent writer owns: {:?}",
            session
                .extension_data
                .extension_states
                .keys()
                .collect::<Vec<_>>()
        );
    }

    /// The inverse: subagents disabled (here: a non-Auto mode) means no
    /// injection and no spawn tool — today's behaviour, preserved.
    #[tokio::test]
    async fn subagents_disabled_injects_nothing() {
        let (agent, session_id) = agent_in_chat_mode_for_tests().await;
        assert!(!agent.subagents_enabled(&session_id).await, "precondition");
        let names: Vec<String> = agent
            .list_tools(&session_id, None)
            .await
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(!names.iter().any(|n| n.contains("subagent")), "{names:?}");
        assert!(
            !names.iter().any(|n| n.starts_with("workspace__")),
            "{names:?}"
        );
    }
}

/// BR-32: the reply loop's stall-check seam — when it runs, when it stays silent,
/// and who owns stall detection when a `/goal` is set.
/// Overflow compaction must use the raw durable prefix, not the provider-only
/// projection that omits historical Bedrock reasoning.
#[cfg(test)]
mod rewrite_basis_tests {
    use super::*;
    use crate::session::session_manager::SessionType;
    use crate::session::SessionManager;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// A session holding `texts`, plus the basis paired with that history.
    async fn seeded(texts: &[&str]) -> (TempDir, SessionManager, String, RewriteBasis) {
        let dir = TempDir::new().unwrap();
        let sm = SessionManager::new(dir.path().to_path_buf());
        let id = sm
            .create_session(PathBuf::from("."), "basis".to_string(), SessionType::User)
            .await
            .unwrap()
            .id;
        for text in texts {
            sm.add_message(&id, &Message::user().with_text(*text))
                .await
                .unwrap();
        }
        let basis = RewriteBasis::read(&sm, &id).await.unwrap();
        (dir, sm, id, basis)
    }

    #[tokio::test]
    async fn raw_overflow_input_keeps_signed_seed_and_appends_only_new_durable_rows() {
        let (_dir, _sm, _id, seed) = seeded(&[]).await;
        let mut signed = Message::assistant()
            .with_thinking("durable reasoning", "durable signature")
            .with_text("durable text  ");
        signed.id = Some("signed-row".to_string());
        let basis = RewriteBasis {
            known: Conversation::new_unvalidated(vec![signed.clone()]),
            revision: seed.revision,
        };

        let mut filtered_same_row = Message::assistant().with_text("durable text");
        filtered_same_row.id = Some("signed-row".to_string());
        let mut new_durable = Message::user().with_text("new durable result");
        new_durable.id = Some("new-row".to_string());
        let ephemeral = Message::user().with_text("ephemeral resource context");
        let live =
            Conversation::new_unvalidated(vec![filtered_same_row, new_durable.clone(), ephemeral]);

        let raw = basis.raw_with_new_durable_messages(&live);
        assert_eq!(raw.messages(), &[signed, new_durable]);
    }
}

#[cfg(test)]
mod stall_seam_tests {
    use super::*;
    use crate::agents::AgentConfig;
    use crate::config::permission::PermissionManager;
    use crate::config::BioRouterMode;
    use crate::model::ModelConfig;
    use crate::providers::base::{ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::SessionType;
    use crate::session::SessionManager;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    /// A judge that always reports a loop, and counts how often it was consulted —
    /// so a test can assert the check did NOT cost a provider round-trip.
    struct LoopyJudge {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Provider for LoopyJudge {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "loopy",
                "Loopy",
                "",
                "loopy-model",
                vec!["loopy-model"],
                "",
                vec![],
            )
        }

        fn get_name(&self) -> &str {
            "loopy"
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok((
                Message::assistant().with_text(
                    r#"{"looping": true, "reason": "the same failing shell command, six times"}"#,
                ),
                ProviderUsage::new("loopy-model".to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("loopy-model")
        }
    }

    /// An agent over an isolated session store, wired to the counting judge.
    async fn agent_with_judge(dir: &std::path::Path) -> (Agent, String, Arc<AtomicUsize>) {
        let session_manager = Arc::new(SessionManager::new(dir.to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        ));
        let session_id = agent
            .config
            .session_manager
            .create_session(PathBuf::from("."), "stall".to_string(), SessionType::User)
            .await
            .unwrap()
            .id;
        let calls = Arc::new(AtomicUsize::new(0));
        agent
            .update_provider(
                Arc::new(LoopyJudge {
                    calls: Arc::clone(&calls),
                }),
                &session_id,
            )
            .await
            .unwrap();
        (agent, session_id, calls)
    }

    fn busy_conversation() -> Conversation {
        let mut conversation = Conversation::default();
        conversation.push(Message::user().with_text("fix the failing build"));
        conversation.push(Message::assistant().with_text("Running the build again."));
        conversation
    }

    #[tokio::test]
    async fn a_normal_turn_never_pays_for_the_check() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        for actions in [1u32, 12, 29] {
            let action = agent
                .stall_check(
                    &session_id,
                    &busy_conversation(),
                    actions,
                    &config,
                    &mut watch,
                )
                .await;
            assert_eq!(action, StallAction::Proceed);
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no provider round-trip before the threshold"
        );
    }

    #[tokio::test]
    async fn a_long_turn_is_checked_and_nudged() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        let action = agent
            .stall_check(&session_id, &busy_conversation(), 30, &config, &mut watch)
            .await;
        match action {
            StallAction::Nudge { reason } => assert!(reason.contains("same failing shell command")),
            other => panic!("expected a nudge, got {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly one check");
    }

    #[tokio::test]
    async fn a_goal_session_keeps_its_own_stall_detector() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        agent
            .set_goal(&session_id, "the build passes".to_string())
            .await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        let action = agent
            .stall_check(&session_id, &busy_conversation(), 30, &config, &mut watch)
            .await;
        assert_eq!(
            action,
            StallAction::Proceed,
            "the goal loop already judges every stop and owns its own stall budget"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "a goal session must not pay for a second loop judge"
        );
    }

    #[tokio::test]
    async fn a_turn_that_gave_up_is_not_re_checked() {
        let dir = TempDir::new().unwrap();
        let (agent, session_id, calls) = agent_with_judge(dir.path()).await;
        let config = StallCheckConfig::default();
        let mut watch = StallWatch::default();

        // Three near-identical looping verdicts (checks at 30/40/50) → give up.
        for actions in [30u32, 40, 50] {
            agent
                .stall_check(
                    &session_id,
                    &busy_conversation(),
                    actions,
                    &config,
                    &mut watch,
                )
                .await;
        }
        assert!(watch.has_given_up());
        let checks_at_giveup = calls.load(Ordering::SeqCst);
        assert_eq!(checks_at_giveup, 3);

        // The wrap-up window must not re-run the judge.
        let action = agent
            .stall_check(&session_id, &busy_conversation(), 60, &config, &mut watch)
            .await;
        assert_eq!(action, StallAction::Proceed);
        assert_eq!(calls.load(Ordering::SeqCst), checks_at_giveup);
    }

    #[tokio::test]
    async fn a_provider_less_agent_fails_open() {
        let dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.path().to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        ));
        let mut watch = StallWatch::default();

        let action = agent
            .stall_check(
                "no-such-session",
                &busy_conversation(),
                30,
                &StallCheckConfig::default(),
                &mut watch,
            )
            .await;
        assert_eq!(action, StallAction::Proceed, "no provider → no verdict");
    }
}

/// #66: the guard that keeps the `MessagesPersisted` ordering invariant a
/// MECHANISM instead of an audited convention.
///
/// The invariant itself, and why it exists, is stated on the `persisted_ordering`
/// seam above. What *this* module gates is the only remaining way to break it:
/// reaching around the seam. Rust's module privacy already makes the frame
/// builder uncallable from out here — that is the real gate, and it is a compile
/// error, not an assertion. These tests cover the two things privacy alone
/// cannot say:
///
///  1. that no site outside the seam so much as names the builder, so the
///     compile error is a fact about the file rather than a fact about what
///     nobody has tried yet; and
///  2. that nobody hand-rolls a [`PersistedMessage`] out here and hands it
///     straight to the frame, which would reproduce the pre-#66 state under a
///     different spelling — privacy on the builder cannot stop that, because the
///     row type has to stay publicly constructible for the server's fixtures.
#[cfg(test)]
mod persisted_ordering_guard {
    /// This file, read as text. Resolved at compile time, so the check cannot
    /// silently start scanning something else.
    const SOURCE: &str = include_str!("agent.rs");

    /// The sentinel comments that bracket the seam. Assembled at runtime for the
    /// same reason the needles below are: spelling either marker literally out
    /// here would move the split and make the whole guard vacuous.
    fn markers() -> (String, String) {
        let stem = concat!("#66 PERSISTED", "-ORDERING-SEAM");
        (format!("// {stem}:BEGIN"), format!("// {stem}:END"))
    }

    /// `(inside the seam, everything else)`.
    ///
    /// With the markers absent the whole file is "everything else", which is the
    /// honest answer when there is no seam — and makes this guard fail loudly on
    /// a tree where the seam was deleted, rather than pass on an empty scan.
    fn split_on_seam(src: &str) -> (String, String) {
        let (begin, end) = markers();
        let Some((before, rest)) = src.split_once(begin.as_str()) else {
            return (String::new(), src.to_string());
        };
        let Some((inside, after)) = rest.split_once(end.as_str()) else {
            return (String::new(), src.to_string());
        };
        (format!("{begin}{inside}{end}"), format!("{before}{after}"))
    }

    /// The private frame builder must have no callers outside the seam.
    ///
    /// The compile error is the gate; this is the statement that the gate is
    /// load-bearing today. A new publication site that reaches for the builder
    /// directly cannot compile, and one that reaches for it *and* moves itself
    /// inside the seam to make it compile trips the surface check below.
    #[test]
    fn nothing_outside_the_seam_calls_the_private_frame_builder() {
        let (seam, outside) = split_on_seam(SOURCE);
        // Assembled at runtime: spelling the identifier literally anywhere in
        // this file — including here — would make the guard pass vacuously.
        let builder = concat!("persisted", "_event(");
        let calls = outside.matches(builder).count();
        assert_eq!(
            calls, 0,
            "{calls} call(s) to the private frame builder live outside the \
             ordering seam. Every publication site must name which of the three \
             legitimate shapes it is: `yielded_then_named`, \
             `named_but_never_yielded` or `named_after_earlier_yield`, so the \
             invariant can be audited by reading the constructor instead of \
             tracing control flow out from it."
        );
        assert!(
            !seam.is_empty(),
            "the ordering seam's sentinel comments are gone; without them this \
             guard scans nothing and passes for the wrong reason"
        );
    }

    /// Nobody builds a published row by hand out here.
    ///
    /// `PersistedMessage`'s fields stay public because the server's SSE and relay
    /// tests construct fixtures from them, so privacy cannot close this door. A
    /// struct literal outside the seam is the one way left to assemble a
    /// `MessagesPersisted` payload without going through a named shape.
    #[test]
    fn nothing_outside_the_seam_hand_rolls_a_published_row() {
        let (_seam, outside) = split_on_seam(SOURCE);
        let literal = concat!("PersistedMessage", " {");
        let hand_rolled = outside.matches(literal).count();
        assert_eq!(
            hand_rolled, 0,
            "{hand_rolled} hand-rolled published row(s) outside the ordering \
             seam. Deriving a row from a `Message` is the seam's job; doing it \
             out here re-opens the exact hole the seam closes."
        );
    }

    /// The seam's public surface is exactly the three shapes.
    ///
    /// Deliberately hard-coded. A fourth escape hatch is not necessarily wrong —
    /// but it is a new claim about when publishing early is safe, and it should
    /// cost an edit here and a reviewer's attention, not slip in as one more
    /// exported function.
    #[test]
    fn the_seam_exposes_only_the_three_named_shapes() {
        let (seam, _outside) = split_on_seam(SOURCE);
        let exported_fn = concat!("pub", "(super) fn ");
        let mut exported: Vec<&str> = seam
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix(exported_fn))
            .map(|rest| {
                rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("")
            })
            .collect();
        exported.sort_unstable();
        assert_eq!(
            exported,
            [
                "named_after_earlier_yield",
                "named_but_never_yielded",
                "yielded_then_named",
            ],
            "the ordering seam grew (or lost) a shape. Each name is a claim \
             about when a `MessagesPersisted` may be emitted; adding one means \
             adding a case to the audit."
        );
    }
}

/// Issue #56, Gate A: the bind refuses a public model on a private session, and
/// it does so in SQL rather than in Rust, so a concurrent ratchet cannot
/// interleave into "private session, public provider bound".
///
/// The two forced-interleaving tests drive the rendezvous points in [`seams`].
/// Both wrap the spawned bind in [`seams::armed`], under the token their own
/// `arm_*` call minted — see the note above those statics for why a bind must
/// opt in, and why the token has to name which seam it may be caught at.
#[cfg(test)]
mod gate_a_bind_tests {
    use super::*;
    use crate::agents::AgentConfig;
    use crate::config::permission::PermissionManager;
    use crate::config::BioRouterMode;
    use crate::model::ModelConfig;
    use crate::privacy::refusal::PrivacyRefusal;
    use crate::privacy::{bind_allowed, ProviderTier, SessionClassification};
    use crate::providers::base::{ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::{Session, SessionType};
    use crate::session::SessionManager;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// A real `Provider` whose only interesting property is its tier. The gate
    /// reads `tier()` and `get_name()`/`get_model_config()`, and nothing here
    /// ever completes a turn.
    struct TieredProvider {
        name: &'static str,
        model: &'static str,
        tier: ProviderTier,
    }

    #[async_trait]
    impl Provider for TieredProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new("tiered", "Tiered", "", "tiered-model", vec![], "", vec![])
        }

        fn get_name(&self) -> &str {
            self.name
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            Ok((
                Message::assistant().with_text("ok"),
                ProviderUsage::new(self.model.to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail(self.model)
        }
    }

    fn private_provider() -> Arc<dyn Provider> {
        Arc::new(TieredProvider {
            name: "versa_azure",
            model: "gpt-5.5",
            tier: ProviderTier::Private,
        })
    }

    fn private_provider2() -> Arc<dyn Provider> {
        Arc::new(TieredProvider {
            name: "ollama",
            model: "qwen3.6",
            tier: ProviderTier::Private,
        })
    }

    fn public_provider() -> Arc<dyn Provider> {
        Arc::new(TieredProvider {
            name: "anthropic",
            model: "claude-opus-4",
            tier: ProviderTier::Public,
        })
    }

    /// An agent over an isolated session store, already bound to `provider`.
    ///
    /// The `TempDir` is returned because dropping it deletes the SQLite file the
    /// agent is still holding; every caller binds it for the test's lifetime.
    /// `Arc<Agent>` because `Agent` is not `Clone` and the race tests hand a
    /// handle to a spawned task.
    async fn agent_on(provider: Arc<dyn Provider>) -> (TempDir, Arc<Agent>, Session) {
        let dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.path().to_path_buf()));
        let agent = Arc::new(Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        )));
        let session = agent
            .config
            .session_manager
            .create_session(PathBuf::from("."), "gate-a".to_string(), SessionType::User)
            .await
            .unwrap();
        agent.update_provider(provider, &session.id).await.unwrap();
        (dir, agent, session)
    }

    fn manager(agent: &Agent) -> Arc<SessionManager> {
        agent.config.session_manager.clone()
    }

    async fn ratchet_to_private(sm: &SessionManager, id: &str) {
        sm.update(id)
            .raise_privacy(SessionClassification::Private, "turn:versa_azure")
            .apply()
            .await
            .unwrap();
    }

    async fn ratchet_to_private_owned(sm: Arc<SessionManager>, id: String) {
        ratchet_to_private(&sm, &id).await;
    }

    async fn reread(sm: &SessionManager, id: &str) -> Session {
        sm.get_session(id, false).await.unwrap()
    }

    fn model_name_of(row: &Session) -> String {
        row.model_config
            .as_ref()
            .expect("a bound session carries a model config")
            .model_name
            .clone()
    }

    #[tokio::test]
    async fn a_public_provider_cannot_be_bound_to_a_private_session() {
        let (_dir, agent, session) = agent_on(private_provider()).await;
        let sm = manager(&agent);
        ratchet_to_private(&sm, &session.id).await;

        let err = agent
            .update_provider(public_provider(), &session.id)
            .await
            .unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<PrivacyRefusal>(),
                Some(PrivacyRefusal::PublicModelOnPrivateSession { .. })
            ),
            "expected a typed privacy refusal, got: {err}"
        );

        // The half that catches the wrong implementation: before this task the
        // in-memory swap PRECEDED the persist. A gate that checks the row but
        // leaves that order alone refuses the write and still leaves the chat
        // running on the public model in memory.
        assert_eq!(agent.provider().await.unwrap().get_name(), "versa_azure");
        // And the row is untouched.
        assert_eq!(
            reread(&sm, &session.id).await.provider_name.as_deref(),
            Some("versa_azure")
        );
    }

    #[tokio::test]
    async fn a_private_provider_binds_to_anything_at_the_agent_layer() {
        let (_dir, agent, s) = agent_on(public_provider()).await;
        // upward: user-only. Deliberately inverted by DR-16 (was `// upward:
        // fine`). The AGENT-level bind stays legal, because it is below the
        // gate: session restore, the CLI and the apps runtime all bind upward
        // legitimately. The gate is one layer up, on the only channel a model
        // can reach — see Task 18A, whose
        // `all_four_raise_channels_call_the_guard` is the assertion that the
        // HTTP raise is refused.
        agent
            .update_provider(private_provider(), &s.id)
            .await
            .unwrap();

        let (_dir2, agent2, s2) = agent_on(private_provider()).await;
        ratchet_to_private(&manager(&agent2), &s2.id).await;
        agent2
            .update_provider(private_provider2(), &s2.id)
            .await
            .unwrap(); // private->private
    }

    #[tokio::test]
    async fn the_sql_predicate_and_bind_allowed_agree_on_every_combination() {
        // `privacy::bind_allowed` reads as Gate A's predicate, but Gate A does
        // not call it — the live gate is the `WHERE` clause, because a predicate
        // evaluated in Rust leaves the window the tests above force a ratchet
        // into. Two spellings of one rule, in two languages, with nothing making
        // them agree: relaxing either alone is silent.
        //
        // It is not a cosmetic drift. `visible_to` (Gate D — which chats a
        // caller may SEE, and which conversations may be ingested) delegates to
        // `bind_allowed`, and the induction in `privacy::tests` uses it as its
        // admission gate. That test says so itself: "whichever task first wires
        // Gate A owes it one, and must not read this test as already covering
        // it." This pays that debt, against the live statement rather than a
        // second copy of the predicate.
        for incoming in [ProviderTier::Public, ProviderTier::Private] {
            for classification in [
                SessionClassification::Public,
                SessionClassification::Private,
            ] {
                let (_dir, agent, s) = agent_on(private_provider()).await;
                let sm = manager(&agent);
                if classification == SessionClassification::Private {
                    ratchet_to_private(&sm, &s.id).await;
                }

                let outcome = sm
                    .storage()
                    .bind_provider_if_allowed(&s.id, "p", "{}", incoming.is_private())
                    .await
                    .unwrap();

                assert_eq!(
                    outcome == BindOutcome::Bound,
                    bind_allowed(incoming, classification),
                    "Gate A's WHERE clause and privacy::bind_allowed disagree for a \
                     {incoming:?} provider on a {classification:?} session: the statement \
                     said {outcome:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_nonexistent_session_is_not_reported_as_a_privacy_refusal() {
        // `rows_affected == 0` means BOTH "the row is private and this model is
        // public" AND "there is no row with that id". Collapsing them is the one
        // way the first test in this module can lie: against a stale fixture id
        // it would pass for entirely the wrong reason, and in production a
        // mistyped id would reach the user as "this chat is private".
        //
        // Asserted at the layer that owns the distinction, which is stronger
        // than a `downcast_ref(..).is_none()` on the caller's error: it names
        // which of the three outcomes was produced.
        let (_dir, agent, s) = agent_on(private_provider()).await;
        let sm = manager(&agent);
        ratchet_to_private(&sm, &s.id).await;

        assert_eq!(
            sm.storage()
                .bind_provider_if_allowed("no-such-session-id", "anthropic", "{}", false)
                .await
                .unwrap(),
            BindOutcome::NoSuchSession
        );
        // …and the same zero, on a row that DOES exist, is the refusal.
        assert_eq!(
            sm.storage()
                .bind_provider_if_allowed(&s.id, "anthropic", "{}", false)
                .await
                .unwrap(),
            BindOutcome::RefusedByPrivacy
        );

        // The caller therefore sees no refusal for a bad id. It is not an error
        // either — see the ⚠ on `update_provider`'s `NoSuchSession` arm.
        agent
            .update_provider(public_provider(), "no-such-session-id")
            .await
            .expect("a bind against an id with no row persists nothing and refuses nothing");
    }

    #[tokio::test]
    async fn a_bind_is_never_accepted_against_a_row_that_is_already_private() {
        // Interleaving (A), FORCED: the ratchet commits strictly BEFORE the
        // bind's UPDATE runs. This is the case the conditional UPDATE exists
        // for, and the one nothing could previously produce.
        //
        // The seam's position is the test. `before_bind_write` is called INSIDE
        // `bind_provider_if_allowed`, as the last statement before `.execute`.
        // Parked there, a `SELECT privacy_tier` + unconditional `UPDATE` helper
        // reads Public, parks, lets the ratchet commit, and then writes anyway
        // — which is the bug, and this assertion is what sees it. Parked before
        // the helper was even entered (its previous position), that same wrong
        // helper would run its SELECT after the ratchet and refuse for a
        // right-looking reason.
        let (_dir, agent, s) = agent_on(private_provider()).await;
        let sm = manager(&agent);

        let reached = seams::arm_before_bind_write();
        let bind = tokio::spawn(seams::armed(reached.token(), {
            let a = Arc::clone(&agent);
            let id = s.id.clone();
            async move { a.update_provider(public_provider(), &id).await }
        }));
        let release = reached.arrived().await; // parked INSIDE the helper, after any read
        ratchet_to_private(&sm, &s.id).await; // runs alone, to completion
        release.send(()).unwrap();

        let err = bind.await.unwrap().unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<PrivacyRefusal>(),
                Some(PrivacyRefusal::PublicModelOnPrivateSession { .. })
            ),
            "the WHERE clause did not see a ratchet that committed before it; \
             either the predicate is evaluated in Rust before the UPDATE, or the \
             seam drifted back out of bind_provider_if_allowed"
        );
        let row = reread(&sm, &s.id).await;
        assert_eq!(
            row.provider_name.as_deref(),
            Some("versa_azure"),
            "a refused bind wrote anyway"
        );
        assert_eq!(agent.provider().await.unwrap().get_name(), "versa_azure");
    }

    #[tokio::test]
    async fn a_ratchet_that_commits_after_a_legal_bind_lands_in_the_state_gate_b_owns() {
        // Interleaving (B), FORCED: the ratchet commits AFTER the bind's UPDATE
        // and BEFORE the in-memory swap. Both statements were legal when they
        // ran, so both succeed and the row ends (private, anthropic).
        //
        // THIS IS NOT A BUG. "The provider bound to a private session is always
        // private" is not a sentence a conditional UPDATE can deliver. What it
        // delivers is narrower and exact: *a bind is never accepted against a
        // row that is already private*. A ratchet landing after a legal bind is
        // a different event, and the state it produces — private row, public
        // `provider_name` — is the SAME residual an LRU rehydration, a legacy
        // row and `restore_provider_from_session`'s `Config::global()` fallback
        // all produce. Task 13's
        // `an_unrepairable_mismatch_refuses_this_turn_and_leaves_the_row_alone`
        // is what owns it, and the repair card is what fixes it.
        let (_dir, agent, s) = agent_on(private_provider()).await;
        let sm = manager(&agent);

        let reached = seams::arm_after_bind_before_swap();
        let bind = tokio::spawn(seams::armed(reached.token(), {
            let a = Arc::clone(&agent);
            let id = s.id.clone();
            async move { a.update_provider(public_provider(), &id).await }
        }));
        let release = reached.arrived().await;
        ratchet_to_private(&sm, &s.id).await;
        release.send(()).unwrap();
        bind.await.unwrap().unwrap(); // the bind was legal when it ran: Ok

        let row = reread(&sm, &s.id).await;
        assert_eq!(row.privacy_tier, SessionClassification::Private);
        assert_eq!(row.provider_name.as_deref(), Some("anthropic"));
        // Not TORN: `provider_name` and `model_config_json` came from one
        // UPDATE, so no reader can see one provider's name beside another's
        // model config.
        assert_eq!(
            model_name_of(&row),
            public_provider().get_model_config().model_name
        );
    }

    #[tokio::test]
    async fn a_bind_armed_for_one_seam_cannot_consume_the_other_seams_arm() {
        // The two tests above run as `#[tokio::test]`s in one binary, on
        // parallel threads, with nothing serialising them — and interleaving
        // (B)'s bind traverses `before_bind_write` on its way to the seam it
        // actually wants, because EVERY bind does: that seam is inside the
        // storage helper. An arm keyed only on "this task is armed" therefore
        // lets (B)'s bind consume (A)'s arm.
        //
        // What that costs is not a visible failure. (A)'s `arrived()` resolves
        // from the wrong task, so (A) runs its ratchet while its OWN bind is
        // unparked and racing it; if the ratchet happens to land first, (A)
        // passes having forced nothing — a silent pass in the one test that
        // exists to force an interleaving. This pins the fix: the token names
        // its seam, and `park` compares before consuming.
        let (_dir, agent, s) = agent_on(private_provider()).await;

        // Interleaving (A)'s arm, placed and never used — it stands in for the
        // other test being mid-flight on another thread.
        let mut before = seams::arm_before_bind_write();
        // …and interleaving (B)'s bind, authorized for the LATER seam only.
        let after = seams::arm_after_bind_before_swap();
        let bind = tokio::spawn(seams::armed(after.token(), {
            let a = Arc::clone(&agent);
            let id = s.id.clone();
            async move { a.update_provider(public_provider(), &id).await }
        }));

        // It must arrive at its own seam. Stealing the other arm parks it at
        // the earlier one instead, and this bounded await says so rather than
        // hanging the binary.
        let release = after.arrived().await;
        release.send(()).unwrap();
        bind.await.unwrap().unwrap();

        assert!(
            !before.has_fired(),
            "a bind armed for after_bind_before_swap announced itself at \
             before_bind_write, so it consumed an arm belonging to another test, \
             whose own bind then runs unforced"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_unconstrained_race_observes_both_outcomes() {
        // The fuzz layer is KEPT — a seam only proves the two interleavings
        // someone thought of. What changes is that it must PROVE it raced.
        //
        // `flavor = "multi_thread"` is load-bearing: `#[tokio::test]` defaults
        // to `current_thread`, where two `tokio::spawn`s cannot preempt each
        // other at all — they interleave only at `.await` points, in the same
        // order every iteration. Two hundred iterations of a deterministic
        // schedule is one iteration, run two hundred times.
        //
        // ⚠ The closing assertion is a claim about a SCHEDULER, and the minority
        // arm is thin — 11, 13 and 20 refusals per 200 on three measured runs
        // here (8 cores, load ~7). A runner that serialises the two spawns
        // harder could go one-sided against a perfectly correct implementation,
        // and that failure reads as a code defect. So the loop runs a FLOOR of
        // 200 iterations always, and then keeps going while it has still seen
        // only one outcome, up to a CEILING. This is NOT an early exit and
        // cannot shorten the fuzz: the per-iteration invariant is checked on
        // every iteration either way, so the ceiling only ever ADDS coverage,
        // on the machines where the floor was not enough.
        const FLOOR: usize = 200;
        const CEILING: usize = 1000;
        let (mut bound, mut refused) = (0usize, 0usize);
        let mut iterations = 0usize;
        while iterations < FLOOR || (iterations < CEILING && (bound == 0 || refused == 0)) {
            iterations += 1;
            let (_dir, agent, s) = agent_on(private_provider()).await;
            let sm = manager(&agent);
            let a = tokio::spawn({
                let a = Arc::clone(&agent);
                let id = s.id.clone();
                async move { a.update_provider(public_provider(), &id).await }
            });
            let b = tokio::spawn(ratchet_to_private_owned(Arc::clone(&sm), s.id.clone()));
            let (a, b) = tokio::join!(a, b);
            b.unwrap();
            let bind_ok = a.unwrap().is_ok();
            if bind_ok {
                bound += 1
            } else {
                refused += 1
            }

            // The invariant that holds in EVERY interleaving, asserted
            // UNCONDITIONALLY — an `if row.is_private()` guard would make the
            // whole assertion skippable, and the ratchet always wins the row, so
            // it would be skipped in the only branch it could have caught.
            let row = reread(&sm, &s.id).await;
            assert_eq!(row.privacy_tier, SessionClassification::Private);
            assert_eq!(
                row.provider_name.as_deref() == Some("anthropic"),
                bind_ok,
                "a refused bind wrote the row, or an accepted one did not"
            );
        }
        assert!(
            bound > 0 && refused > 0,
            "{iterations} iterations produced {bound} bound / {refused} refused, which is one-sided, so \
             the loop raced nothing. That is the state this test used to report as a pass."
        );
    }
}

/// Issue #56 Gate B — the turn barrier, the ratchet, and Gate B' on the
/// completions that never pass through `reply`.
///
/// The four wrong implementations these tests exist to reject, one each:
///
///  1. A refuse-only Gate B. The residual state (a private row whose live agent
///     holds a public provider) is produced by LRU rehydration, by
///     `restore_provider_from_session`'s `Config::global()` fallback, by a
///     legacy row, and by any ratchet that commits after a legal bind. Refusing
///     all of them bricks the majority of sessions on a private machine. The
///     row still names a provider that satisfies the classification, so the
///     repair is a silent rebind FROM THE ROW.
///  2. A gate at the literal top of `reply`. The prologue has early returns
///     before any provider contact, and one of them delivers a user's answer to
///     a parked elicitation. Refusing there drops the answer.
///  3. A ratchet on the BIND. Then a mis-click privatises a chat, and
///     `POST /agent/call_tool` — which never binds — is missed entirely.
///  4. A gate that lives only in `reply`. Session auto-naming, compaction
///     summarisation and the stall judge each read the whole transcript through
///     `complete_fast` without ever entering `reply`.
#[cfg(test)]
mod gate_b_turn_tests {
    use super::*;
    use crate::agents::AgentConfig;
    use crate::config::permission::PermissionManager;
    use crate::config::BioRouterMode;
    use crate::model::ModelConfig;
    use crate::privacy::{ProviderTier, SessionClassification};
    use crate::providers::base::{ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::{Session, SessionType};
    use crate::session::SessionManager;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    /// The one sentence every turn refusal contains. Spelled here as a literal
    /// rather than imported, so that a change to `turn_refusal`'s wording that
    /// silently stopped refusing would still have to get past these tests;
    /// `privacy::refusal`'s own unit test asserts the marker is present.
    const REFUSAL_MARKER: &str = "this turn was not sent";

    /// A provider whose interesting properties are its tier and how many
    /// completions it has been asked for. The count is what test 5 reads: Gate
    /// B' is only meaningful if the transcript never reaches the model, and
    /// "no error was returned" does not establish that.
    struct CountingProvider {
        name: &'static str,
        model: &'static str,
        tier: ProviderTier,
        completions: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Provider for CountingProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "counting",
                "Counting",
                "",
                "counting-model",
                vec![],
                "",
                vec![],
            )
        }

        fn get_name(&self) -> &str {
            self.name
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            self.completions.fetch_add(1, Ordering::SeqCst);
            Ok((
                Message::assistant().with_text("ok"),
                ProviderUsage::new(self.model.to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail(self.model)
        }
    }

    fn counted(
        name: &'static str,
        model: &'static str,
        tier: ProviderTier,
    ) -> (Arc<dyn Provider>, Arc<AtomicUsize>) {
        let completions = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(CountingProvider {
            name,
            model,
            tier,
            completions: Arc::clone(&completions),
        });
        (provider, completions)
    }

    fn private_provider() -> Arc<dyn Provider> {
        counted("versa_azure", "gpt-5.5", ProviderTier::Private).0
    }

    fn public_provider() -> Arc<dyn Provider> {
        counted("anthropic", "claude-opus-4", ProviderTier::Public).0
    }

    /// An agent over an isolated session store, already bound to `provider`.
    /// The `TempDir` outlives the test because dropping it deletes the SQLite
    /// file the agent still holds.
    async fn agent_on(provider: Arc<dyn Provider>) -> (TempDir, Arc<Agent>, Session) {
        let dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.path().to_path_buf()));
        let agent = Arc::new(Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        )));
        let session = agent
            .config
            .session_manager
            .create_session(PathBuf::from("."), "gate-b".to_string(), SessionType::User)
            .await
            .unwrap();
        agent.update_provider(provider, &session.id).await.unwrap();
        (dir, agent, session)
    }

    fn manager(agent: &Agent) -> Arc<SessionManager> {
        agent.config.session_manager.clone()
    }

    /// Point the ROW at `provider` without touching the live agent's binding —
    /// which is exactly the residual state Gate B has to deal with, and which
    /// no ordinary call can produce because `update_provider` does both halves.
    /// Goes through Gate A's own statement, so the fixture cannot construct a
    /// state the production bind path would have refused.
    async fn point_row_at(sm: &SessionManager, id: &str, provider: &Arc<dyn Provider>) {
        let model_config_json = serde_json::to_string(&provider.get_model_config()).unwrap();
        let outcome = sm
            .storage()
            .bind_provider_if_allowed(
                id,
                provider.get_name(),
                &model_config_json,
                provider.tier().is_private(),
            )
            .await
            .unwrap();
        assert_eq!(
            outcome,
            BindOutcome::Bound,
            "the fixture's own bind was refused"
        );
    }

    async fn ratchet_to_private(sm: &SessionManager, id: &str) {
        sm.update(id)
            .raise_privacy(SessionClassification::Private, "turn:versa_azure")
            .apply()
            .await
            .unwrap();
    }

    async fn reread(sm: &SessionManager, id: &str) -> Session {
        sm.get_session(id, false).await.unwrap()
    }

    fn cfg(session: &Session) -> SessionConfig {
        SessionConfig {
            id: session.id.clone(),
            schedule_id: None,
            max_turns: Some(2),
            max_tool_calls: None,
            budget: None,
            retry_config: None,
            reasoning_effort: None,
        }
    }

    async fn drain(mut stream: BoxStream<'_, Result<AgentEvent>>) -> Vec<Result<AgentEvent>> {
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }
        events
    }

    fn is_refusal(event: &Result<AgentEvent>) -> bool {
        match event {
            Ok(AgentEvent::Message(message)) => message.as_concat_text().contains(REFUSAL_MARKER),
            _ => false,
        }
    }

    fn rendered(events: &[Result<AgentEvent>]) -> String {
        events
            .iter()
            .map(|event| match event {
                Ok(AgentEvent::Message(m)) => format!("Message({:?})", m.as_concat_text()),
                Ok(other) => format!("{other:?}"),
                Err(e) => format!("Err({e})"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn a_repairable_mismatch_rebinds_silently_and_the_turn_runs() {
        // The residual state: privacy_tier=private, live agent holds a public
        // provider (LRU rehydration, the Config::global() fallback, a legacy
        // row). The row still names a private provider, so Gate B rebinds FROM
        // THE ROW and continues — the user never sees it. An implementation
        // that only refuses fails this, and it is the majority case on a real
        // machine.
        let (_dir, agent, s) = agent_on(public_provider()).await;
        let sm = manager(&agent);
        let row_provider = private_provider();
        point_row_at(&sm, &s.id, &row_provider).await;
        ratchet_to_private(&sm, &s.id).await;
        seams::override_rebind_provider(&s.id, "versa_azure", Arc::clone(&row_provider));

        let events = drain(
            agent
                .reply(Message::user().with_text("hi"), cfg(&s), None)
                .await
                .unwrap(),
        )
        .await;
        assert!(
            !events.iter().any(is_refusal),
            "a repairable mismatch must not be refused:\n{}",
            rendered(&events)
        );
        assert_eq!(agent.provider().await.unwrap().get_name(), "versa_azure");
    }

    #[tokio::test]
    async fn an_unrepairable_mismatch_refuses_this_turn_and_leaves_the_row_alone() {
        let (_dir, agent, s) = agent_on(public_provider()).await;
        let sm = manager(&agent);
        // The row names a PUBLIC provider, so there is nothing to repair to.
        // The override is registered anyway, so the construction step is still
        // hermetic and the refusal comes from the tier check rather than from
        // a factory that happened to fail for a credential reason.
        let row_provider = public_provider();
        point_row_at(&sm, &s.id, &row_provider).await;
        ratchet_to_private(&sm, &s.id).await;
        seams::override_rebind_provider(&s.id, "anthropic", Arc::clone(&row_provider));

        let events = drain(
            agent
                .reply(Message::user().with_text("hi"), cfg(&s), None)
                .await
                .unwrap(),
        )
        .await;
        assert!(
            events.iter().any(is_refusal),
            "an unrepairable mismatch must be refused:\n{}",
            rendered(&events)
        );
        // A refusal, not a 500: the stream yields and returns.
        assert!(
            events.iter().all(|e| e.is_ok()),
            "the refusal must be a yielded message, never an Err out of `reply`:\n{}",
            rendered(&events)
        );
        let row = reread(&sm, &s.id).await;
        assert_eq!(row.privacy_tier, SessionClassification::Private);
        assert_eq!(row.provider_name.as_deref(), Some("anthropic"));
    }

    #[tokio::test]
    async fn an_elicitation_answer_is_still_delivered_on_a_private_session() {
        // The seam matters: at the literal top of `reply` this refuses, and the
        // user's answer to a parked tool call is silently dropped.
        let (_dir, agent, s) = agent_on(public_provider()).await;
        let sm = manager(&agent);
        let row_provider = public_provider();
        point_row_at(&sm, &s.id, &row_provider).await;
        ratchet_to_private(&sm, &s.id).await;
        seams::override_rebind_provider(&s.id, "anthropic", row_provider);

        let answer =
            Message::user().with_content(MessageContent::action_required_elicitation_response(
                "elicit-1",
                serde_json::json!({"answer": "yes"}),
            ));
        let events = drain(agent.reply(answer, cfg(&s), None).await.unwrap()).await;
        assert!(
            !events.iter().any(is_refusal),
            "an elicitation answer is a user action on a parked tool call, not a \
             disclosure; the gate sits after this early return:\n{}",
            rendered(&events)
        );
    }

    #[tokio::test]
    async fn the_first_turn_ratchets_and_a_permitted_bind_afterwards_is_refused() {
        let (_dir, agent, s) = agent_on(private_provider()).await;
        let sm = manager(&agent);
        // The bind did NOT ratchet (O5): a mis-clicked model switch must not
        // privatise a chat, and a ratchet there would still miss every turn
        // that arrives through `POST /agent/call_tool`.
        assert_eq!(
            reread(&sm, &s.id).await.privacy_tier,
            SessionClassification::Public
        );

        let _ = drain(
            agent
                .reply(Message::user().with_text("hi"), cfg(&s), None)
                .await
                .unwrap(),
        )
        .await;

        let row = reread(&sm, &s.id).await;
        assert_eq!(row.privacy_tier, SessionClassification::Private);
        assert_eq!(row.privacy_reason.as_deref(), Some("turn:versa_azure"));
        assert!(agent
            .update_provider(public_provider(), &s.id)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn auto_naming_a_private_transcript_on_a_public_provider_is_refused() {
        // Gate B'. `maybe_rename_session` -> `maybe_update_name` ->
        // `generate_session_name` -> `complete_fast` reads the entire
        // transcript and never passes `reply`. Same for the stall judge and
        // for the SYNCHRONOUS half of compaction summarisation, which reach
        // the binding through the same accessor. Compaction's background half
        // does not — see the test below it, which covers that one separately.
        //
        // The swap is done directly on the shared `Arc` rather than through
        // `update_provider`, on purpose: `update_provider` is Gate A, and a
        // test that went through it would be testing Gate A a third time.
        let (_dir, agent, s) = agent_on(private_provider()).await;
        let _ = drain(
            agent
                .reply(Message::user().with_text("hi"), cfg(&s), None)
                .await
                .unwrap(),
        )
        .await; // ratchets
        assert_eq!(
            reread(&manager(&agent), &s.id).await.privacy_tier,
            SessionClassification::Private
        );

        let (public, public_completions) =
            counted("anthropic", "claude-opus-4", ProviderTier::Public);
        *agent.provider.lock().await = Some(public);

        agent.maybe_rename_session(&s.id).await;
        assert_eq!(
            public_completions.load(Ordering::SeqCst),
            0,
            "the session's whole transcript was sent to a public model to be named"
        );
    }

    /// Is a background compaction registered for this session right now?
    fn eager_compaction_in_flight(agent: &Agent, id: &str) -> bool {
        agent
            .eager_compactions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(id)
    }

    #[tokio::test]
    async fn background_compaction_of_a_private_transcript_on_a_public_provider_is_refused() {
        // Gate B' lives in `Agent::provider`, but `maybe_spawn_eager_compaction`
        // cannot call it — it must not block on the provider lock — so it clones
        // the `SharedProvider` directly and asserts the predicate inline. That
        // makes it the one bypass of the accessor inside a path Gate B' NAMES as
        // covered, and this is the test that keeps it covered.
        //
        // The in-flight marker is the observable, and on a current-thread
        // runtime it discriminates in both directions: a refusal clears it
        // synchronously before returning, while a spawned task cannot be polled
        // until the next await, so a permitted call leaves it set. There is no
        // await between either call and its assertion, so the background task
        // cannot interleave and clear the marker underneath us.
        let (_dir, agent, s) = agent_on(private_provider()).await;
        let sm = manager(&agent);
        ratchet_to_private(&sm, &s.id).await;
        agent
            .cached_classification
            .store(&s.id, SessionClassification::Private);

        // Permitted: a private provider on a private session spawns. This half
        // is not decoration — it is what stops the refusal assertion below from
        // passing vacuously on a machine where `BIOROUTER_EAGER_COMPACT=false`,
        // because a disabled feature returns before the marker is ever set.
        agent.maybe_spawn_eager_compaction(&cfg(&s), std::path::Path::new("."));
        assert!(
            eager_compaction_in_flight(&agent, &s.id),
            "eager compaction did not spawn for the PERMITTED case, so the \
             refusal assertion below would prove nothing"
        );
        agent.clear_eager_compaction(&s.id);

        // Refused: a public provider swapped in behind Gate B's back — the LRU
        // rehydration residual, arriving between two turns.
        *agent.provider.lock().await = Some(public_provider());
        agent.maybe_spawn_eager_compaction(&cfg(&s), std::path::Path::new("."));
        assert!(
            !eager_compaction_in_flight(&agent, &s.id),
            "a public provider was handed a private transcript to summarise in \
             the background"
        );
    }
}

#[cfg(test)]
mod gate_c_dispatch_tests {
    //! Issue #56 Gate C, the caller half: every production path that reaches
    //! `ExtensionManager::dispatch_tool_call` must surface the refusal to
    //! whoever asked, in the caller's own error surface.
    //!
    //! FOUR paths converge on that function and only ONE of them carries a
    //! `ToolInspector` — which is why Gate C is a branch inside the manager
    //! rather than an inspector, and why one agent-loop test would not have
    //! caught an inspector-shaped implementation. Three are exercised here:
    //!
    //! | # | path | exercised by |
    //! |---|---|---|
    //! | 1 | the agent loop (`Agent::dispatch_tool_call`) | `call_private_tool_via_agent_loop` |
    //! | 2 | `POST /agent/call_tool` | `call_private_tool_as_the_http_route_does` |
    //! | 3 | the `execute_code` JS bridge | `code_execution_extension::gate_c_bridge_tests` |
    //! | 4 | `Agent::call_prefetch_tool`, which runs BEFORE the turn | `call_private_tool_via_call_prefetch_tool` |
    //!
    //! Path 3 lives beside its own function because `dispatch_sub_call` is
    //! private to `code_execution_extension` and Rust does not let a sibling
    //! module call it. Path 2's route handler lives in another crate; what is
    //! asserted here is the capability it hands the manager
    //! (`Public` + enforced) and the refusal that comes back, and that the
    //! handler renders it rather than swallowing it is asserted in
    //! `biorouter-server`'s `routes::agent::gate_c_call_tool_tests`.

    use super::*;
    use crate::agents::AgentConfig;
    use crate::config::permission::PermissionManager;
    use crate::config::BioRouterMode;
    use crate::model::ModelConfig;
    use crate::privacy::ProviderTier;
    use crate::providers::base::{ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::{Session, SessionType};
    use crate::session::SessionManager;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// One of the two extensions the compiled-in BAAM baseline calls private.
    const PRIVATE_EXTENSION: &str = "ucsfomopagent";
    const PRIVATE_TOOL: &str = "ucsfomopagent__data_sources";

    struct PlainProvider {
        name: &'static str,
        tier: ProviderTier,
    }

    #[async_trait]
    impl Provider for PlainProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new("plain", "Plain", "", "plain-model", vec![], "", vec![])
        }

        fn get_name(&self) -> &str {
            self.name
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        /// ⚠ **A private double must state an affiliation, because every real
        /// private provider does** — DR-26, Task 48.
        ///
        /// `Some(..)` exactly while a provider's tier is Private is a property
        /// of this build, not an accident: both deciders route *through* the
        /// tier predicate (`ucsf_gateway_affiliation`, `self_hosted_affiliation`)
        /// and `LeadWorkerProvider` folds both halves. Leaving this on the trait
        /// default produced the one pairing DR-26's vocabulary says cannot exist
        /// — Private tier, affiliation `None` — which
        /// `CallCapability::cross_affiliation_warning` treats as *unstated*
        /// rather than as *unconstrained*, and rightly: reading `None` as "no
        /// institution applies" is the fail-open this axis exists to prevent.
        ///
        /// `Local` rather than an institution, so these tests stay about the
        /// TIER axis they were written for: it is DR-26's identity element, the
        /// one model affiliation compatible with every extension. `self.name`
        /// is a provider NAME and never decides an affiliation — see
        /// `Provider::affiliation`'s doc for why a name-keyed table is wrong.
        fn affiliation(&self) -> Option<crate::privacy::ModelAffiliation> {
            match self.tier {
                ProviderTier::Private => Some(crate::privacy::ModelAffiliation::Local),
                ProviderTier::Public => None,
            }
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            Ok((
                Message::assistant().with_text("ok"),
                ProviderUsage::new("plain-model".to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("plain-model")
        }
    }

    fn public_provider() -> Arc<dyn Provider> {
        Arc::new(PlainProvider {
            name: "anthropic",
            tier: ProviderTier::Public,
        })
    }

    /// An agent bound to `provider`, over an isolated session store, with the
    /// private extension already loaded. The `TempDir` outlives the test
    /// because dropping it deletes the SQLite file the agent still holds.
    async fn agent_with_the_private_extension(
        provider: Arc<dyn Provider>,
    ) -> (TempDir, Arc<Agent>, Session) {
        let dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let permission_manager = Arc::new(PermissionManager::new(dir.path().to_path_buf()));
        let agent = Arc::new(Agent::with_config(AgentConfig::new(
            session_manager,
            permission_manager,
            None,
            BioRouterMode::Auto,
        )));
        let session = agent
            .config
            .session_manager
            .create_session(PathBuf::from("."), "gate-c".to_string(), SessionType::User)
            .await
            .unwrap();
        agent.update_provider(provider, &session.id).await.unwrap();
        // A real in-process MCP server admitted under a private NAME, so the
        // tier the gate reads is stamped by the production admission path
        // rather than poked into the record by the fixture.
        agent
            .extension_manager
            .add_inprocess_server(
                PRIVATE_EXTENSION,
                biorouter_mcp::datasql::server::DataSqlServer::new(std::collections::HashMap::new()),
            )
            .await
            .expect("inject the private extension");
        (dir, agent, session)
    }

    fn call(name: &str) -> CallToolRequestParams {
        CallToolRequestParams {
            task: None,
            name: name.to_string().into(),
            arguments: Some(rmcp::object!({})),
            meta: None,
        }
    }

    /// Path 1: the agent loop. `Agent::dispatch_tool_call` samples the
    /// capability from the bound (public) provider and hands it down.
    async fn call_private_tool_via_agent_loop() -> String {
        let (_dir, agent, session) = agent_with_the_private_extension(public_provider()).await;
        let (_id, result) = agent
            .dispatch_tool_call(call(PRIVATE_TOOL), "req-1".to_string(), None, &session)
            .await;
        match result
            .expect("the agent loop wraps a dispatch refusal as a tool result")
            .result
            .await
        {
            Ok(ok) => panic!("a public model reached a private extension: {ok:?}"),
            Err(e) => e.message.to_string(),
        }
    }

    /// Path 2: `POST /agent/call_tool`. It arrives with no caller identity, so
    /// it hands the manager the most restrictive pair — the value the route's
    /// own constructor returns — built here with the test constructor so the
    /// census of the two production spellings keeps counting production entries
    /// only (`tests/privacy_capability.rs` pins that the two agree).
    ///
    /// ⚠ Do not spell that constructor here **in code**. Task 51's census
    /// (`the_sites_that_decide_how_far_a_caller_reaches_are_exactly_these`) greps
    /// `crates/*/src/` for its literal name followed by `(` and asserts the exact
    /// (file, count) set — one production entry, in the route itself. A test
    /// spelling it would be indistinguishable from a second entry nobody
    /// classified, which is the one thing that check exists to catch. Prose is
    /// safe: the census skips `//` lines, for the reason `grant.rs`'s twin audit
    /// records — an audit that reads comments goes red over a sentence, and
    /// teaches the next person to relax it.
    async fn call_private_tool_as_the_http_route_does() -> String {
        let (_dir, agent, session) = agent_with_the_private_extension(public_provider()).await;
        // `ToolCallResult` is not `Debug`, so the outcome is matched rather
        // than `expect_err`'d.
        match agent
            .extension_manager
            .dispatch_tool_call(
                &session.id,
                call(PRIVATE_TOOL),
                crate::privacy::CallCapability::for_test(ProviderTier::Public, true),
                CancellationToken::default(),
            )
            .await
        {
            Ok(_) => panic!("an entry with no caller identity reached a private extension"),
            Err(e) => e.to_string(),
        }
    }

    /// Path 4: the pre-turn prefetch, which dispatches outside
    /// `Agent::dispatch_tool_call` entirely and runs BEFORE the turn — so an
    /// inspector-shaped gate would never see it.
    async fn call_private_tool_via_call_prefetch_tool() -> String {
        let (_dir, agent, session) = agent_with_the_private_extension(public_provider()).await;
        let err = agent
            .call_prefetch_tool(&session.id, PRIVATE_TOOL, serde_json::Map::new())
            .await
            .expect_err("the prefetch is a dispatch like any other");
        err.to_string()
    }

    #[tokio::test]
    async fn every_convergent_path_into_the_manager_is_refused() {
        // Three separate assertions, one per production path reachable from
        // this crate's `agents` module. A single agent-loop test passes an
        // implementation written as a `ToolInspector`, which paths 2 and 4
        // bypass entirely.
        let text_from_agent_loop = call_private_tool_via_agent_loop().await;
        let text_from_http_call_tool = call_private_tool_as_the_http_route_does().await;
        let text_from_prefetch = call_private_tool_via_call_prefetch_tool().await;

        // The WHOLE refusal, not merely the extension's name: `Tool
        // 'ucsfomopagent__data_sources' not found` also contains the name, so a
        // substring assertion on it alone would pass on a fixture that never
        // loaded the extension — and would go on passing after Gate C was
        // deleted.
        let refusal = crate::privacy::refusal::privacy_refusal(
            PRIVATE_EXTENSION,
            ProviderTier::Private,
            ProviderTier::Public,
        )
        .expect("the pure refusal")
        .message
        .to_string();

        for t in [
            text_from_agent_loop,
            text_from_http_call_tool,
            text_from_prefetch,
        ] {
            assert!(
                t.contains(&refusal),
                "refusal did not reach the caller intact: {t}"
            );
            assert!(
                !t.contains("The user has declined"),
                "laundered as a decline: {t}"
            );
        }
    }

    /// The other direction, so the three assertions above cannot be satisfied
    /// by a gate that refuses everything: the same extension, the same tool,
    /// the same agent loop, on a private model, runs.
    #[tokio::test]
    async fn a_private_model_still_reaches_the_private_extension() {
        let private: Arc<dyn Provider> = Arc::new(PlainProvider {
            name: "versa_azure",
            tier: ProviderTier::Private,
        });
        let (_dir, agent, session) = agent_with_the_private_extension(private).await;
        let (_id, result) = agent
            .dispatch_tool_call(call(PRIVATE_TOOL), "req-2".to_string(), None, &session)
            .await;
        result
            .expect("dispatch")
            .result
            .await
            .expect("a private model may call a private extension");
    }

    /// Issue #56 Task 48, DR-26 — **the bind surface**, driven through the real
    /// `Agent::update_provider` rather than through a mutex write.
    ///
    /// Binding a model covered by one institution's agreements into a chat
    /// already holding another institution's connector is the same mismatch the
    /// enable path finds from the opposite end. It **warns**: unlike Gate A's
    /// tier refusal it does not block, because both endpoints are Private,
    /// legitimate cross-institutional work under a real DUA exists, and a
    /// blocked-outright design is one researchers route around by turning the
    /// feature off (DR-19).
    ///
    /// ⚠ The bind is asserted to SUCCEED before the warning is read. A version
    /// of this test that only checked the warning would pass on an
    /// implementation that had turned the bind into a refusal.
    #[tokio::test]
    async fn binding_a_foreign_institutions_model_warns_and_still_binds() {
        let local: Arc<dyn Provider> = Arc::new(PlainProvider {
            name: "ollama",
            tier: ProviderTier::Private,
        });
        let (_dir, agent, session) = agent_with_the_private_extension(local).await;
        assert!(
            agent.cross_affiliation_warnings().await.is_empty(),
            "a local model reaches everything private, so no transfer occurs at all"
        );

        let elsewhere: Arc<dyn Provider> = Arc::new(ProviderCoveredBy {
            tier: ProviderTier::Private,
            affiliation: Some(crate::privacy::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new("stanford"),
            )),
        });
        agent
            .update_provider(elsewhere, &session.id)
            .await
            .expect("a mismatch warns; it must never refuse the bind");

        let warnings = agent.cross_affiliation_warnings().await;
        assert_eq!(
            warnings.len(),
            1,
            "exactly the UCSF connector mismatches: {warnings:?}"
        );
        assert_eq!(warnings[0].0, PRIVATE_EXTENSION);
        assert!(warnings[0].1.contains("ucsf"), "{}", warnings[0].1);
        assert!(warnings[0].1.contains("stanford"), "{}", warnings[0].1);

        // Issue #56 Task 52 (DR-27) — **the same bind, on a machine whose user
        // asked for cross-institution reach to be silent**, driven end to end
        // rather than at the pure gate.
        //
        // Two claims, and the second is the one that keeps `open` from becoming
        // the master switch in miniature: the STATEMENT goes quiet, and the
        // RESOLUTION does not. `/agent/add_extension` logs this same sentence
        // from the other end and already goes quiet in `open`; a bind that went
        // on speaking would be the second place to disagree that the
        // single-reader design exists to prevent.
        {
            let _pin =
                crate::privacy::mixing::pin_for_test(crate::privacy::mixing::MixingPolicy::Open);
            assert!(
                agent.cross_affiliation_warnings().await.is_empty(),
                "`open` still stated a cross-institution warning at the bind, while the \
                 enable path's identical statement goes quiet"
            );
            assert!(
                agent
                    .cross_affiliation_grant_subject(PRIVATE_EXTENSION)
                    .await
                    .is_some(),
                "`open` short-circuited the resolver. Gate E's mark, the badges and the \
                 grant route's subject all read through it, and `open -> standard` would \
                 then have nothing to re-tighten"
            );
        }
        // …and it comes straight back when the pin drops, which is that
        // re-tightening.
        assert_eq!(
            agent.cross_affiliation_warnings().await.len(),
            1,
            "re-tightening did not restore the statement"
        );
    }

    /// Issue #56, the "warn the user" half of DR-26 that shipped as a log line:
    /// [`Agent::cross_affiliation_notice`], the body the bind and enable routes
    /// hand back to the person who just acted.
    ///
    /// The test above proves the daemon *knows* about the mismatch. This one
    /// proves the sentence a surface can actually show exists, says both
    /// institutions, and respects the acceptance the user already gave — the
    /// three ways this can be wrong that a caller cannot check for itself.
    ///
    /// ⚠ **Step 1 is the control that keeps the rest honest.** Without it every
    /// assertion below is satisfied by a composer that returns the empty string
    /// whenever it is confused, which is the failure mode of a privacy statement
    /// nobody sees.
    #[tokio::test]
    async fn the_bind_notice_names_both_institutions_and_goes_quiet_once_accepted() {
        let ucsf = Some(crate::privacy::ModelAffiliation::institution(
            crate::privacy::affiliation::InstitutionId::new("ucsf"),
        ));
        let stanford = Some(crate::privacy::ModelAffiliation::institution(
            crate::privacy::affiliation::InstitutionId::new("stanford"),
        ));
        let mayo = Some(crate::privacy::ModelAffiliation::institution(
            crate::privacy::affiliation::InstitutionId::new("mayo"),
        ));

        // 1. The approved arrangement says nothing. A notice that spoke here
        //    would train every user to dismiss it.
        let (_dir, agent, session) = agent_with_the_private_extension(covered_by("ucsf")).await;
        assert_eq!(
            agent.cross_affiliation_notice(&session.id, ucsf).await,
            "",
            "a model covered by the connector's own institution crosses no boundary"
        );

        // 2. Bound to another institution's model: the notice is the statement,
        //    and it names BOTH ends. Naming only one is the version of this a
        //    user cannot act on.
        agent
            .update_provider(covered_by("stanford"), &session.id)
            .await
            .expect("a mismatch warns; it must never refuse the bind");
        let notice = agent.cross_affiliation_notice(&session.id, stanford).await;
        assert!(
            notice.contains(PRIVATE_EXTENSION),
            "the notice must name the connector the user has to decide about: {notice}"
        );
        assert!(
            notice.contains("UCSF (ucsf)"),
            "the institution that owns the connector's data: {notice}"
        );
        assert!(
            notice.contains("stanford"),
            "the institution whose agreements cover the bound model: {notice}"
        );

        // 3. The user accepts that exact flow at a dispatch. The bind surface
        //    must then stop repeating a boundary the daemon has agreed to let
        //    them cross — otherwise Settings and the model picker nag about a
        //    decision that has already been made.
        crate::privacy::grant::record_for_test(
            &agent.config.session_manager,
            &session.id,
            PRIVATE_EXTENSION,
            stanford,
        )
        .await
        .expect("the user's acceptance is recorded against this chat");
        assert_eq!(
            agent.cross_affiliation_notice(&session.id, stanford).await,
            "",
            "the notice repeated a flow the user has already accepted"
        );

        // 4. …and a THIRD institution is a different flow, so the acceptance
        //    does not carry over. Dropping this axis would turn one yes into a
        //    standing permission that survives a model switch nobody reviewed —
        //    the same axis step 5 of the end-to-end test guards at the dispatch.
        agent
            .update_provider(covered_by("mayo"), &session.id)
            .await
            .expect("a mismatch warns; it must never refuse the bind");
        let after_switch = agent.cross_affiliation_notice(&session.id, mayo).await;
        assert!(
            after_switch.contains("mayo"),
            "an acceptance for one institution silenced the warning for another: \
             {after_switch:?}"
        );
    }

    /// Issue #56: the finding this notice exists to fix was **a correct query
    /// with no user-facing caller**, so the composer is worthless without one and
    /// this is the assertion that says so.
    ///
    /// ⚠ **It reads the daemon's real source, not a copy.** The two surfaces the
    /// ruling names are HTTP routes in another crate, which no unit test in this
    /// crate can drive; what can be checked mechanically is that both of them
    /// still ask. Deleting either call — the exact regression, since the routes
    /// worked for years while only logging — turns this red.
    #[test]
    fn the_notice_is_read_by_both_warn_and_proceed_routes() {
        let routes = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("crates/biorouter-server/src/routes/agent.rs");
        let src = std::fs::read_to_string(&routes).unwrap_or_else(|e| {
            panic!(
                "the routes that surface DR-26's bind statement are missing at {} ({e})",
                routes.display()
            )
        });
        // Assembled, so this assertion is not itself the match a copy of this
        // test in the routes file would find.
        let needle = concat!("cross_affiliation", "_notice(");
        let callers = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains(needle))
            .count();
        assert_eq!(
            callers, 2,
            "`Agent::cross_affiliation_notice` is read by {callers} routes, not by both \
             warn-and-proceed surfaces. DR-26 requires the user be told at the bind \
             (`POST /agent/update_provider`) AND at their own enable \
             (`POST /agent/add_extension`); a composer with no caller is exactly the \
             defect this method was added to fix, and it fails silently: the daemon \
             keeps logging and the user keeps seeing nothing."
        );
    }

    /// The wire detail neither side can check alone: the daemon joins the
    /// warnings and the renderer splits them, and if the two ever spell the
    /// separator differently **nothing fails** — two statements about two
    /// different pairs of institutions render as one run-together paragraph, or
    /// one statement is silently split in half.
    ///
    /// Modelled on `privacy::grant::tests::
    /// the_scope_copy_the_user_reads_is_the_one_the_daemon_records`, which exists
    /// for the same class of silent drift, and it has to live on the Rust side
    /// for the same reason: the renderer's tests cannot see this constant.
    #[test]
    fn the_renderer_splits_the_notice_the_daemon_joins() {
        assert_eq!(
            Agent::CROSS_AFFILIATION_NOTICE_SEPARATOR,
            "\n\n",
            "the daemon changed how it joins warnings without the renderer being told"
        );
        let mirror = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("ui/desktop/src/utils/crossAffiliationNotice.ts");
        let src = std::fs::read_to_string(&mirror).unwrap_or_else(|e| {
            panic!(
                "the renderer's notice module is missing at {} ({e}). Without it the bind and \
                 enable surfaces are back to logging a warning nobody sees, which is the whole \
                 of this fix.",
                mirror.display()
            )
        });
        // The escaped SPELLING, because the value itself is two newlines and a
        // search for that matches every blank line in the file.
        assert!(
            src.contains(r"CROSS_AFFILIATION_NOTICE_SEPARATOR = '\n\n'"),
            "the renderer no longer splits on the separator the daemon joins with, so a \
             multi-warning notice renders as one confused claim. Re-mirror \
             `Agent::CROSS_AFFILIATION_NOTICE_SEPARATOR` into {}.",
            mirror.display()
        );
    }

    /// A provider at a stated tier and affiliation. [`PlainProvider`] derives
    /// its affiliation from its tier and so can only ever be `Local`, which is
    /// DR-26's identity element — a fixture built from it can never produce a
    /// mismatch.
    struct ProviderCoveredBy {
        tier: ProviderTier,
        affiliation: Option<crate::privacy::ModelAffiliation>,
    }

    #[async_trait]
    impl Provider for ProviderCoveredBy {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "covered",
                "Covered",
                "",
                "covered-model",
                vec![],
                "",
                vec![],
            )
        }

        fn get_name(&self) -> &str {
            "covered-by"
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        fn affiliation(&self) -> Option<crate::privacy::ModelAffiliation> {
            self.affiliation
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            Ok((
                Message::assistant().with_text("ok"),
                ProviderUsage::new("covered-model".to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("covered-model")
        }
    }

    /// A provider covered by `institution`'s agreements, private tier.
    fn covered_by(institution: &str) -> Arc<dyn Provider> {
        Arc::new(ProviderCoveredBy {
            tier: ProviderTier::Private,
            affiliation: Some(crate::privacy::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new(institution),
            )),
        })
    }

    /// The private tool, called through the agent loop, expecting a refusal.
    async fn refusal_for(agent: &Arc<Agent>, session: &Session, request_id: &str) -> String {
        let (_id, result) = agent
            .dispatch_tool_call(call(PRIVATE_TOOL), request_id.to_string(), None, session)
            .await;
        match result
            .expect("the agent loop wraps a dispatch refusal as a tool result")
            .result
            .await
        {
            Ok(ok) => panic!("a cross-institutional dispatch was permitted: {ok:?}"),
            Err(e) => e.message.to_string(),
        }
    }

    /// **Issue #56 Task 51, Step 3 — the operator's scenario, end to end, in one
    /// test.** Five moves in one chat, through the real agent: the approved flow
    /// runs, the cross-institutional one is refused with a warning naming both
    /// institutions, the user accepts it, the same call then proceeds, and
    /// re-binding to a third institution's model does not inherit that
    /// acceptance.
    ///
    /// ⚠ **It is driven through `Agent::update_provider` and
    /// `Agent::dispatch_tool_call`, not by handing gates a hand-built
    /// capability.** The grant's own dispatch tests
    /// (`extension_manager::…::a_granted_triple_permits_the_dispatch_and_re_binding_does_not_reuse_it`)
    /// pass their capability in, so they cannot fail if `sample` stops reading
    /// `p.affiliation()` at all; the bind test above binds for real but never
    /// dispatches. Nothing joined the two until this. The chain it exercises is
    /// bind → sample → Gate C → grant, every link of it live.
    ///
    /// ⚠ **The roles of the two institutions are mirrored from DR-26's wording,
    /// because this build cannot express the other arrangement.** The ruling
    /// describes a UCSF-covered Versa model refused at *another* institution's
    /// connector; `ucsf` is the only institution the compiled registry snapshot
    /// publishes (`privacy::registry_private::INSTITUTIONS`, and
    /// `providers::factory::tests::
    /// every_institution_a_provider_claims_is_published_by_the_registry` is what
    /// keeps a provider from claiming one that is not there), so a Stanford-owned
    /// extension is not constructible and the foreign endpoint has to be the
    /// model. The
    /// mismatch is the same one — DR-26's table is symmetric in which side is
    /// foreign — and move 1 keeps the half that matters most: the arrangement
    /// everyone approved still runs.
    ///
    /// Without move 1 every assertion below is satisfied by a gate that refuses
    /// everything, which is the design DR-26 explicitly rejects.
    #[tokio::test]
    async fn the_operators_cross_institutional_scenario_end_to_end() {
        // 1. The UCSF-covered model reaching the UCSF OMOP agent: the
        //    arrangement everyone approved. No warning, and the call runs.
        let (_dir, agent, session) = agent_with_the_private_extension(covered_by("ucsf")).await;
        assert!(
            agent.cross_affiliation_warnings().await.is_empty(),
            "a model covered by the connector's own institution crosses no boundary"
        );
        let (_id, approved) = agent
            .dispatch_tool_call(call(PRIVATE_TOOL), "req-ucsf".to_string(), None, &session)
            .await;
        approved
            .expect("dispatch")
            .result
            .await
            .expect("UCSF's model may reach UCSF's connector: this is the approved flow");

        // 2. Re-bound to another institution's model. The bind WARNS and still
        //    succeeds (DR-19 on the third axis), and the very same call is now
        //    refused with a statement naming both institutions.
        agent
            .update_provider(covered_by("stanford"), &session.id)
            .await
            .expect("a mismatch warns; it must never refuse the bind");
        let refused = refusal_for(&agent, &session, "req-stanford").await;
        assert!(refused.contains(PRIVATE_EXTENSION), "{refused}");
        assert!(
            refused.contains("UCSF (ucsf)"),
            "the institution that owns the connector's data: {refused}"
        );
        assert!(
            refused.contains("stanford"),
            "the institution whose agreements cover the bound model: {refused}"
        );

        // 3. The user accepts that stated risk, through the module's own test
        //    door. Nothing on a dispatch path can reach the real one: the proof
        //    of user is `X-User-Action`, an HTTP header with no channel here,
        //    which is exactly why the flow is refuse → tell the user → grant over
        //    HTTP → retry.
        crate::privacy::grant::record_for_test(
            &agent.config.session_manager,
            &session.id,
            PRIVATE_EXTENSION,
            Some(crate::privacy::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new("stanford"),
            )),
        )
        .await
        .expect("the user's acceptance is recorded against this chat");

        // 4. …and the identical call now proceeds. A grant that changed nothing
        //    would leave DR-26 a blanket block.
        let (_id, granted) = agent
            .dispatch_tool_call(
                call(PRIVATE_TOOL),
                "req-granted".to_string(),
                None,
                &session,
            )
            .await;
        granted
            .expect("dispatch")
            .result
            .await
            .expect("the user accepted this exact flow, so the next call proceeds");

        // 5. Re-bound to a THIRD institution's model: same chat, same connector,
        //    different third axis. The triple the user accepted no longer exists,
        //    so the grant does not reach this call — the axis an implementer is
        //    most likely to drop, and dropping it turns a one-time acceptance
        //    into a standing permission that survives a model switch nobody
        //    reviewed.
        agent
            .update_provider(covered_by("mayo"), &session.id)
            .await
            .expect("a mismatch warns; it must never refuse the bind");
        let refused_again = refusal_for(&agent, &session, "req-mayo").await;
        assert!(
            refused_again.contains("mayo"),
            "the refusal names the newly bound institution: {refused_again}"
        );
        assert!(
            refused_again.contains("UCSF (ucsf)"),
            "…and still the one that owns the data: {refused_again}"
        );
        assert!(
            !refused_again.contains("stanford"),
            "the accepted flow was about the model that is no longer bound: {refused_again}"
        );
    }
}
