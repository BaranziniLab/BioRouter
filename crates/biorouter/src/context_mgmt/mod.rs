use crate::config::Config;
use crate::conversation::message::{ActionRequiredData, MessageMetadata};
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::{merge_consecutive_messages, Conversation};
use crate::prompt_template::render_global_file;
use crate::providers::base::{Provider, ProviderUsage};
use crate::providers::errors::ProviderError;
use anyhow::Result;
use rmcp::model::{Role, Tool};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub mod pins;

pub use pins::{
    pin_is_eligible, select_pins, EvictedPin, PinEvictionReason, PinLimits, PinSelection,
    DEFAULT_MAX_PINNED_CONTEXT_SHARE, DEFAULT_MAX_PINNED_MESSAGES,
};

pub const DEFAULT_COMPACTION_THRESHOLD: f64 = 0.8;

/// BR-14: a usable compaction summary must clear this many characters. Below it
/// the "summary" is effectively empty (a refusal, a stray token, a blank
/// response) and is retried instead of silently becoming the agent's memory.
const MIN_SUMMARY_CHARS: usize = 40;

/// BR-14: how many of the mandated sections a summary must name to count as
/// well-formed. Deliberately lenient — a stronger model may merge or rename a
/// heading, and a false retry costs a whole extra summarization call.
const MIN_SUMMARY_SECTIONS: usize = 3;

/// BR-14: the section headings `summarize_oneshot.md` mandates. A well-formed
/// summary names most of them; a total format collapse (a refusal, a one-liner)
/// names none, which — together with [`MIN_SUMMARY_CHARS`] — is what a retry
/// targets. `Next Step` is intentionally excluded: the prompt marks it
/// conditional ("include only if directly continues user instruction").
const MANDATED_SUMMARY_SECTIONS: [&str; 8] = [
    "User Intent",
    "Technical Concepts",
    "Files",
    "Errors",
    "Problem Solving",
    "User Messages",
    "Pending Tasks",
    "Current Work",
];

/// BR-10: number of most-recent turns kept verbatim at compaction; only the
/// older prefix is summarized. Overridable with `BIOROUTER_COMPACT_KEEP_LAST_TURNS`;
/// set it to `0` to restore the legacy summarize-everything behaviour.
pub const DEFAULT_COMPACT_KEEP_LAST_TURNS: usize = 4;

/// BR-12: default state of eager (background, between-turns) compaction. When on,
/// a turn that ends over the compaction threshold kicks off a `tokio::spawn`
/// compaction that swaps in the summarized history before the *next* turn, so
/// the user never waits on a summarization LLM round-trip. `BIOROUTER_EAGER_COMPACT=false`
/// restores the pre-BR-12 synchronous-only path (compaction blocks the next turn).
pub const DEFAULT_EAGER_COMPACTION_ENABLED: bool = true;

/// BR-11: approximate characters per token, used only to turn the summarizer
/// model's token-based context limit into a character budget for the truncation
/// fallback in `do_compact`. English prose is ~4 chars/token; this is a
/// deliberate under-estimate — the summarizer still guards every attempt with
/// `ContextLengthExceeded`, so guessing low only costs one extra retry, never a
/// broken request.
const APPROX_CHARS_PER_TOKEN: usize = 4;

/// Continuation note for the recent-turn window path: the summary covers only
/// the older prefix, and the most recent turns follow it verbatim.
const RECENT_WINDOW_CONTINUATION_TEXT: &str =
    "The message above is a summary of the earlier part of this conversation, which was condensed because a context limit was reached.
The most recent turns follow it verbatim.
Do not mention that you read a summary or that summarization occurred.
Continue the conversation naturally, relying on the summary for older context and the verbatim turns for recent detail.";

const CONVERSATION_CONTINUATION_TEXT: &str =
    "The previous message contains a summary that was prepared because a context limit was reached.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context";

const TOOL_LOOP_CONTINUATION_TEXT: &str =
    "The previous message contains a summary that was prepared because a context limit was reached.
Do not mention that you read a summary or that conversation summarization occurred.
Continue calling tools as necessary to complete the task.";

const MANUAL_COMPACT_CONTINUATION_TEXT: &str =
    "The previous message contains a summary that was prepared at the user's request.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context";

#[derive(Serialize)]
struct SummarizeContext {
    messages: String,
}

/// Compact messages by summarizing them
///
/// This function performs the actual compaction by summarizing messages and updating
/// their visibility metadata. It does not check thresholds - use `check_if_compaction_needed`
/// first to determine if compaction is necessary.
///
/// # Arguments
/// * `provider` - The provider to use for summarization
/// * `conversation` - The current conversation history
/// * `manual_compact` - If true, this is a manual compaction (don't preserve user message)
///
/// # Returns
/// * A tuple containing:
///   - `Conversation`: The compacted messages
///   - `ProviderUsage`: Provider usage from summarization
pub async fn compact_messages(
    provider: &dyn Provider,
    conversation: &Conversation,
    manual_compact: bool,
) -> Result<(Conversation, ProviderUsage)> {
    let keep_last_turns = Config::global()
        .get_param::<usize>("BIOROUTER_COMPACT_KEEP_LAST_TURNS")
        .unwrap_or(DEFAULT_COMPACT_KEEP_LAST_TURNS);
    compact_messages_with_window(provider, conversation, manual_compact, keep_last_turns).await
}

/// BR-13: a reactive context-overflow recovery strategy, in ascending order of
/// aggressiveness. The agent loop steps through these on successive
/// `ContextLengthExceeded` errors (see [`overflow_recovery_for_attempt`]) so a
/// session that stays over-window degrades gracefully instead of dead-ending
/// after two attempts (the old 2-attempt cliff).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowRecovery {
    /// Standard compaction: summarize the older prefix, keep the recent verbatim
    /// window (BR-10 default). The gentlest step; matches the pre-BR-13
    /// first-attempt behaviour.
    Compact,
    /// Summarize more: shrink the verbatim window to `keep_last_turns` so more of
    /// the recent tail is summarized away.
    ShrinkWindow(usize),
    /// Summarize everything — no verbatim window survives, so even a large recent
    /// tool result is collapsed into the summary (and head/tail-truncated by
    /// BR-11 if it alone overflows the summarizer).
    SummarizeAll,
    /// Last resort before giving up: hard-drop the oldest agent-visible turns and
    /// then summarize everything that remains, so even a pathological history that
    /// is still over-window after full summarization can be recovered.
    DropOldestThenSummarize,
}

/// BR-13: map a 1-based reactive-compaction attempt to a recovery strategy, or
/// `None` when the ladder is exhausted and the loop should finally give up.
/// Successive overflows escalate: keep-window → shrink-window → summarize-all →
/// drop-oldest. Returning `None` (attempt 5+) is the only path that surfaces the
/// "context limit still exceeded" error to the user, replacing the old hard
/// 2-attempt cliff.
pub fn overflow_recovery_for_attempt(attempt: usize) -> Option<OverflowRecovery> {
    match attempt {
        1 => Some(OverflowRecovery::Compact),
        2 => Some(OverflowRecovery::ShrinkWindow(2)),
        3 => Some(OverflowRecovery::SummarizeAll),
        4 => Some(OverflowRecovery::DropOldestThenSummarize),
        _ => None,
    }
}

/// BR-13: run one reactive compaction using the given recovery strategy. Reactive
/// compaction is never user-initiated, so `manual_compact` is always false.
pub async fn compact_messages_with_recovery(
    provider: &dyn Provider,
    conversation: &Conversation,
    recovery: OverflowRecovery,
) -> Result<(Conversation, ProviderUsage)> {
    match recovery {
        OverflowRecovery::Compact => {
            let keep_last_turns = Config::global()
                .get_param::<usize>("BIOROUTER_COMPACT_KEEP_LAST_TURNS")
                .unwrap_or(DEFAULT_COMPACT_KEEP_LAST_TURNS);
            compact_messages_with_window(provider, conversation, false, keep_last_turns).await
        }
        OverflowRecovery::ShrinkWindow(keep_last_turns) => {
            compact_messages_with_window(provider, conversation, false, keep_last_turns).await
        }
        OverflowRecovery::SummarizeAll => {
            compact_messages_with_window(provider, conversation, false, 0).await
        }
        OverflowRecovery::DropOldestThenSummarize => {
            let pruned = Conversation::new_unvalidated(drop_oldest_agent_visible_turns(
                conversation.messages(),
            ));
            compact_messages_with_window(provider, &pruned, false, 0).await
        }
    }
}

/// A compaction turn boundary: an agent-visible, text-only user message (a
/// genuine user prompt, not a tool response). Snapping a cut to such a boundary
/// guarantees tool_request/tool_response pairs stay intact across it and that the
/// kept slice never opens with a dangling tool response. Shared by
/// [`recent_window_split`] (BR-10) and [`drop_oldest_agent_visible_turns`] (BR-13).
fn is_user_prompt_turn(msg: &Message) -> bool {
    msg.is_agent_visible()
        && matches!(msg.role, Role::User)
        && msg
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::Text(_)))
        && !msg.content.iter().any(|c| {
            matches!(
                c,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            )
        })
}

/// Return the index into `messages` at which the recent verbatim window begins,
/// or `None` when the recent-turn strategy does not apply — either it is
/// disabled (`keep_last_turns == 0`), there are not more than `keep_last_turns`
/// user-prompt turns, or the older prefix has nothing agent-visible to summarize.
fn recent_window_split(messages: &[Message], keep_last_turns: usize) -> Option<usize> {
    if keep_last_turns == 0 {
        return None;
    }

    let boundaries: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_user_prompt_turn(msg))
        .map(|(idx, _)| idx)
        .collect();

    // Need strictly more turns than we intend to keep, so the older prefix is
    // non-empty and there is something to summarize.
    if boundaries.len() <= keep_last_turns {
        return None;
    }

    let split = boundaries[boundaries.len() - keep_last_turns];

    // The older prefix must contain something agent-visible to summarize; a
    // prefix of only already-invisible messages (e.g. originals from a prior
    // compaction) would yield an empty/junk summary, so fall back instead.
    if messages[..split].iter().any(|m| m.is_agent_visible()) {
        Some(split)
    } else {
        None
    }
}

/// BR-13: hard-drop the oldest agent-visible turns by flipping them
/// agent-invisible — they stay in the user transcript but leave both the agent
/// context and the summarizer input. Used only as the final reactive-compaction
/// stage ([`OverflowRecovery::DropOldestThenSummarize`]) for the pathological
/// case where summarizing the entire history is *still* over-window. Keeps
/// roughly the newer half of the user-prompt turns; the cut snaps to a clean
/// user-prompt boundary so tool_request/tool_response pairs are never split.
///
/// #51: an eligible preservation marker is spared. This runs BEFORE the
/// compaction that honours pins, and it works by flipping messages
/// agent-invisible — so without this clause the bottom rung of the recovery
/// ladder would quietly make every old pin ineligible on its way past, and the
/// compaction that follows would never see a pin to honour.
///
/// Eligibility, not the budget, is the test here: the budget belongs to the
/// compaction proper. Sparing a pin the budget later evicts is harmless (the
/// eviction demotes it back to an ordinary message, which is exactly what this
/// pass would have done); dropping one the budget would have honoured is not.
fn drop_oldest_agent_visible_turns(messages: &[Message]) -> Vec<Message> {
    let boundaries: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| is_user_prompt_turn(msg))
        .map(|(idx, _)| idx)
        .collect();

    // Need at least two user-prompt boundaries to drop the older half and still
    // keep a newer one; with fewer there is nothing safe to drop.
    if boundaries.len() < 2 {
        return messages.to_vec();
    }

    let cut = boundaries[boundaries.len() / 2];

    messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            if idx < cut && msg.is_agent_visible() && !pin_is_eligible(msg) {
                msg.clone()
                    .with_metadata(msg.metadata.with_agent_invisible())
            } else {
                msg.clone()
            }
        })
        .collect()
}

async fn compact_messages_with_window(
    provider: &dyn Provider,
    conversation: &Conversation,
    manual_compact: bool,
    keep_last_turns: usize,
) -> Result<(Conversation, ProviderUsage)> {
    info!("Performing message compaction");

    let messages = conversation.messages();

    // #51: decide ONCE, against this exact slice, which preservation markers
    // this compaction honours — both branches below index into it, and the
    // eviction (if any) is logged exactly once per compaction rather than once
    // per branch.
    let pins = select_pins(
        messages,
        PinLimits::from_config(provider.get_model_config().context_limit()),
    );
    pins.log();

    // BR-10: keep the most recent turns verbatim and summarize only the older
    // prefix, snapping the cut to a clean user-turn boundary.
    //
    // #51: the windowed path only applies if the older prefix still has
    // something to summarize once the honoured pins are taken out of it. A
    // prefix of nothing but pins would otherwise hand `do_compact` an empty (or
    // pins-only) payload and buy a billed round-trip for a summary of nothing.
    let window_split = recent_window_split(messages, keep_last_turns).filter(|&split| {
        messages[..split]
            .iter()
            .enumerate()
            .any(|(idx, msg)| msg.is_agent_visible() && !pins.is_honoured(idx))
    });

    if let Some(split) = window_split {
        let (prefix, kept) = messages.split_at(split);

        // The summarizer never sees an honoured pin. Its text survives verbatim
        // a few messages further down, so summarizing it as well would spend
        // the window on the same content twice.
        let to_summarize: Vec<Message> = prefix
            .iter()
            .enumerate()
            .filter(|(idx, _)| !pins.is_honoured(*idx))
            .map(|(_, msg)| msg.clone())
            .collect();

        let (summary_message, summarization_usage) = do_compact(provider, &to_summarize).await?;

        let mut final_messages: Vec<Message> = Vec::with_capacity(messages.len() + 4);

        // Older prefix: keep in the transcript for the user, hide from the
        // agent — EXCEPT an honoured pin, which stays exactly where it is, with
        // its visibility and its marker untouched so the next compaction
        // honours it too.
        for (idx, msg) in prefix.iter().enumerate() {
            if pins.is_honoured(idx) {
                final_messages.push(msg.clone());
            } else {
                let updated_metadata = msg.metadata.with_agent_invisible();
                final_messages.push(msg.clone().with_metadata(updated_metadata));
            }
        }

        // Summary of the older prefix (agent-only) + a continuation note.
        let summary_msg = summary_message.with_metadata(MessageMetadata::agent_only());
        let continuation_msg = Message::assistant()
            .with_text(RECENT_WINDOW_CONTINUATION_TEXT)
            .with_metadata(MessageMetadata::agent_only());
        let (merged_continuation, _issues) =
            merge_consecutive_messages(vec![summary_msg, continuation_msg]);
        final_messages.extend(merged_continuation);

        // #51: a preservation marker the budget could not honour is announced
        // to both audiences, in the transcript, right where it happened.
        final_messages.extend(pins.eviction_notices());

        // Recent turns: kept verbatim (visibility untouched).
        for msg in kept {
            final_messages.push(msg.clone());
        }

        return Ok((
            Conversation::new_unvalidated(final_messages),
            summarization_usage,
        ));
    }

    let has_text_only = |msg: &Message| {
        let has_text = msg
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::Text(_)));
        let has_tool_content = msg.content.iter().any(|c| {
            matches!(
                c,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            )
        });
        has_text && !has_tool_content
    };

    let extract_text = |msg: &Message| -> Option<String> {
        let text_parts: Vec<String> = msg
            .content
            .iter()
            .filter_map(|c| {
                if let MessageContent::Text(text) = c {
                    Some(text.text.clone())
                } else {
                    None
                }
            })
            .collect();

        if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        }
    };

    // #51: the whole point of the legacy path is that EVERY message is flipped
    // agent-invisible, so it is the path a preservation marker most needs to be
    // honoured on. An honoured pin is excluded from the summarizer input and
    // keeps its metadata untouched below.
    let messages_to_compact: Vec<Message> = messages
        .iter()
        .enumerate()
        .filter(|(idx, _)| !pins.is_honoured(*idx))
        .map(|(_, msg)| msg.clone())
        .collect();

    // Nothing left to summarize once the pins are set aside: every remaining
    // message is either an honoured pin or already hidden. Summarizing here
    // would buy a billed round-trip for a summary of nothing and hand the model
    // a "context was summarized" note that is false. Return the conversation
    // untouched instead — the pins are already verbatim, which is the outcome a
    // compaction would have had to produce anyway.
    if !messages_to_compact.iter().any(|msg| msg.is_agent_visible()) {
        warn!(
            "#51: nothing to summarize — every agent-visible message is either a \
             preserved message or already hidden; leaving the conversation as it is"
        );
        let mut final_messages = messages.clone();
        final_messages.extend(pins.eviction_notices());
        return Ok((
            Conversation::new_unvalidated(final_messages),
            ProviderUsage::new(
                provider.get_model_config().model_name.clone(),
                crate::providers::base::Usage::default(),
            ),
        ));
    }

    // Find and preserve the most recent user message for non-manual compacts.
    // An honoured pin is skipped here: it already survives verbatim in place, so
    // re-appending a fresh copy would duplicate it (and the copy would carry no
    // marker, so the duplicate would then be summarized away next time).
    let (preserved_user_message, is_most_recent) = if !manual_compact {
        let found_msg = messages.iter().enumerate().rev().find(|(idx, msg)| {
            msg.is_agent_visible()
                && matches!(msg.role, rmcp::model::Role::User)
                && has_text_only(msg)
                && !pins.is_honoured(*idx)
        });

        if let Some((idx, msg)) = found_msg {
            let is_last = idx == messages.len() - 1;
            (Some(msg.clone()), is_last)
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };

    let (summary_message, summarization_usage) = do_compact(provider, &messages_to_compact).await?;

    // Create the final message list with updated visibility metadata:
    // 1. Original messages become user_visible but not agent_visible
    // 2. Summary message becomes agent_visible but not user_visible
    // 3. Assistant messages to continue the conversation are also agent_visible but not user_visible
    // 4. #51: an honoured pin is left exactly as it was — this is the only
    //    message the legacy path does not hide.
    let mut final_messages = Vec::new();

    for (idx, msg) in messages.iter().enumerate() {
        if pins.is_honoured(idx) {
            final_messages.push(msg.clone());
            continue;
        }
        let updated_metadata =
            if is_most_recent && idx == messages.len() - 1 && preserved_user_message.is_some() {
                // This is the most recent message and we're preserving it by adding a fresh copy
                MessageMetadata::invisible()
            } else {
                msg.metadata.with_agent_invisible()
            };
        let updated_msg = msg.clone().with_metadata(updated_metadata);
        final_messages.push(updated_msg);
    }

    let summary_msg = summary_message.with_metadata(MessageMetadata::agent_only());

    let mut continuation_messages = vec![summary_msg];

    let continuation_text = if manual_compact {
        MANUAL_COMPACT_CONTINUATION_TEXT
    } else if is_most_recent {
        CONVERSATION_CONTINUATION_TEXT
    } else {
        TOOL_LOOP_CONTINUATION_TEXT
    };

    let continuation_msg = Message::assistant()
        .with_text(continuation_text)
        .with_metadata(MessageMetadata::agent_only());
    continuation_messages.push(continuation_msg);

    let (merged_continuation, _issues) = merge_consecutive_messages(continuation_messages);
    final_messages.extend(merged_continuation);

    // #51: same announcement as the windowed path, so no compaction path can
    // drop a preserved message quietly.
    final_messages.extend(pins.eviction_notices());

    if let Some(user_msg) = preserved_user_message {
        if let Some(text) = extract_text(&user_msg) {
            final_messages.push(Message::user().with_text(&text));
        }
    }

    Ok((
        Conversation::new_unvalidated(final_messages),
        summarization_usage,
    ))
}

/// BR-56: bytes per token for the compaction *trigger* estimate.
///
/// A deliberate **floor**, so the estimate over-counts: o200k averages ~4 bytes
/// per token on English prose and ~3.3 on code/JSON, and even CJK (3 UTF-8 bytes
/// per character, roughly one token per character) lands at ~3. Dividing by 3
/// therefore yields an upper-ish bound, and the fast path only *skips* the exact
/// count when even that upper bound is under the threshold — so the heuristic can
/// cost a needless exact count but cannot hide a needed compaction. Override with
/// `BIOROUTER_COMPACT_ESTIMATE_BYTES_PER_TOKEN`.
const BYTES_PER_TOKEN_FLOOR: f64 = 3.0;

/// Per-message and per-tool overheads mirroring [`TokenCounter::count_chat_tokens`]
/// so the estimate is comparable to the exact count it short-circuits.
const ESTIMATE_TOKENS_PER_MESSAGE: usize = 4;

fn fast_estimate_enabled() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_COMPACT_FAST_ESTIMATE")
        .unwrap_or(true)
}

fn bytes_per_token() -> f64 {
    let configured = Config::global()
        .get_param::<f64>("BIOROUTER_COMPACT_ESTIMATE_BYTES_PER_TOKEN")
        .unwrap_or(BYTES_PER_TOKEN_FLOOR);
    if configured > 0.0 {
        configured
    } else {
        BYTES_PER_TOKEN_FLOOR
    }
}

/// The byte size of one message's model-visible payload.
///
/// Shared by the compaction TRIGGER estimate and the #51 pinned-set BUDGET, so
/// the two can never disagree about how big a message is — a pinned set that
/// looked affordable to one and unaffordable to the other would make the
/// compaction outcome depend on which check ran.
fn message_payload_bytes(message: &Message) -> usize {
    let mut bytes = 0;
    for content in &message.content {
        match content {
            MessageContent::Text(text) => bytes += text.text.len(),
            MessageContent::Thinking(thinking) => bytes += thinking.thinking.len(),
            MessageContent::RedactedThinking(redacted) => bytes += redacted.data.len(),
            MessageContent::Image(image) => bytes += image.data.len(),
            MessageContent::ToolRequest(request) => {
                bytes += request.id.len();
                if let Ok(call) = &request.tool_call {
                    bytes += call.name.len();
                    bytes += call
                        .arguments
                        .as_ref()
                        .map(|args| serde_json::to_string(args).map(|s| s.len()).unwrap_or(0))
                        .unwrap_or(0);
                }
            }
            MessageContent::ToolResponse(response) => {
                bytes += response.id.len();
                if let Ok(result) = &response.tool_result {
                    for item in result.content.iter() {
                        if let Some(text) = item.as_text() {
                            bytes += text.text.len();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    bytes
}

/// The byte-based token estimate for a SINGLE message, including its per-message
/// overhead. What the #51 pinned-set budget is measured in.
fn estimate_message_tokens(message: &Message) -> usize {
    ((message_payload_bytes(message) as f64) / bytes_per_token()).ceil() as usize
        + ESTIMATE_TOKENS_PER_MESSAGE
}

/// A byte-count-based token estimate for the whole request (system prompt +
/// agent-visible messages + tool schemas). No tokenizer, no allocation per
/// message — it walks the text that is already in memory.
fn estimate_request_tokens(messages: &[Message], system_prompt: &str, tools: &[Tool]) -> usize {
    let mut bytes = system_prompt.len();
    let mut overhead = if system_prompt.is_empty() {
        0
    } else {
        ESTIMATE_TOKENS_PER_MESSAGE
    };

    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        overhead += ESTIMATE_TOKENS_PER_MESSAGE;
        bytes += message_payload_bytes(message);
    }

    for tool in tools {
        bytes += tool.name.len();
        bytes += tool.description.as_ref().map(|d| d.len()).unwrap_or(0);
        bytes += serde_json::to_string(&tool.input_schema)
            .map(|s| s.len())
            .unwrap_or(0);
        overhead += ESTIMATE_TOKENS_PER_MESSAGE;
    }

    ((bytes as f64) / bytes_per_token()).ceil() as usize + overhead
}

/// Check if messages exceed the auto-compaction threshold
///
/// `cold_path_context` supplies the system prompt and tool schemas for the
/// fallback token estimate that runs when the provider hasn't reported usage
/// yet (a session's first turn, or providers that never report usage). They are
/// frequently the two largest contributors to a request, so omitting them
/// systematically undercounts and lets such a session slip past the real
/// context window without compacting (BR-15). Pass `None` only when they are
/// genuinely unavailable — the estimate then falls back to messages-only.
pub async fn check_if_compaction_needed(
    provider: &dyn Provider,
    conversation: &Conversation,
    threshold_override: Option<f64>,
    session: &crate::session::Session,
    cold_path_context: Option<(&str, &[Tool])>,
) -> Result<bool> {
    let messages = conversation.messages();
    let config = Config::global();
    let threshold = threshold_override.unwrap_or_else(|| {
        config
            .get_param::<f64>("BIOROUTER_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD)
    });

    let context_limit = provider.get_model_config().context_limit();

    // Auto-compaction disabled: nothing below can change the answer, and the cold
    // path would otherwise pay for a full tokenization to reach the same `false`.
    if threshold <= 0.0 || threshold >= 1.0 {
        return Ok(false);
    }
    let threshold_tokens = (context_limit as f64 * threshold) as usize;

    let (current_tokens, token_source) = match session.total_tokens {
        Some(tokens) => (tokens as usize, "session metadata"),
        None => {
            // BR-15: the fallback estimate must include the system prompt and tool
            // schemas — frequently the two largest contributors — or it undercounts
            // badly. Fall back to empty (the old behaviour) only when the caller
            // couldn't supply them. For a toolshim model the tools are folded into
            // the system prompt and the tool list is empty; both are passed through
            // verbatim, so the count is correct either way.
            let (system_prompt, tools): (String, Vec<Tool>) = match cold_path_context {
                Some((system, tools)) => (system.to_string(), tools.to_vec()),
                None => (String::new(), Vec::new()),
            };

            // o200k under-counts non-OpenAI tokenizers, so inflate the raw
            // estimate by a small, conservative per-provider margin to keep the
            // trigger calibrated across providers.
            let calibration = crate::token_counter::provider_token_calibration(provider.get_name());

            // BR-56: a cheap byte-based upper estimate first. The exact count is a
            // tiktoken BPE encode of the whole request — megabytes of history on a
            // long session, and this runs at the top of every reply for any provider
            // that doesn't report usage. When the estimate says we are *clearly*
            // under the threshold, the exact answer cannot be "compact", so skip it.
            // The estimate deliberately over-counts (see `BYTES_PER_TOKEN_FLOOR`), so
            // this can only ever cost an unnecessary exact count, never a missed
            // compaction.
            if fast_estimate_enabled() {
                let estimate = ((estimate_request_tokens(messages, &system_prompt, &tools) as f64)
                    * calibration)
                    .ceil() as usize;
                if estimate <= threshold_tokens {
                    debug!(
                        "Compaction check short-circuited: byte estimate {} <= threshold {} ({} tokens limit)",
                        estimate, threshold_tokens, context_limit
                    );
                    return Ok(false);
                }
            }

            let token_counter = crate::token_counter::shared_token_counter()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create token counter: {}", e))?;

            // tiktoken BPE over the whole history is CPU-bound. This runs at the
            // top of reply() on a session's first turn (or for providers that
            // don't report usage). Offload it to the blocking pool so it never
            // stalls an async runtime worker. Clone the agent-visible messages to
            // move into the task (cold path only — the happy path uses the
            // provider-reported `session.total_tokens` above). Count once over the
            // whole request rather than per message so the +3 reply primer is only
            // added a single time.
            let owned: Vec<Message> = messages
                .iter()
                .filter(|m| m.is_agent_visible())
                .cloned()
                .collect();
            let total = tokio::task::spawn_blocking(move || {
                let raw = token_counter.count_chat_tokens(&system_prompt, &owned, &tools);
                ((raw as f64) * calibration).ceil() as usize
            })
            .await
            .map_err(|e| anyhow::anyhow!("Token counting task failed: {}", e))?;

            (total, "estimated")
        }
    };

    let usage_ratio = current_tokens as f64 / context_limit as f64;

    // Auto-compact-disabled thresholds already returned above.
    let needs_compaction = usage_ratio > threshold;

    debug!(
        "Compaction check: {} / {} tokens ({:.1}%), threshold: {:.1}%, needs compaction: {}, source: {}",
        current_tokens,
        context_limit,
        usage_ratio * 100.0,
        threshold * 100.0,
        needs_compaction,
        token_source
    );

    Ok(needs_compaction)
}

/// BR-12: outcome of a background eager-compaction attempt. Returned for logging
/// and tests; the caller (a detached `tokio::spawn`) does nothing with it beyond
/// tracing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EagerCompactionOutcome {
    /// Post-turn usage was under the threshold, so nothing was compacted.
    NotNeeded,
    /// The older prefix was summarized and the compacted history swapped in.
    Swapped,
    /// The conversation changed between the snapshot and the swap (a new turn
    /// began while the summarizer ran), so the stale result was discarded. The
    /// next turn's synchronous fallback will compact instead.
    Aborted,
}

/// BR-12: is eager (background, between-turns) compaction enabled? Default on
/// ([`DEFAULT_EAGER_COMPACTION_ENABLED`]); `BIOROUTER_EAGER_COMPACT=false`
/// restores the pre-BR-12 synchronous-only path.
pub fn eager_compaction_enabled() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_EAGER_COMPACT")
        .unwrap_or(DEFAULT_EAGER_COMPACTION_ENABLED)
}

/// BR-12: compute and swap in a compaction off the user-visible critical path.
///
/// Spawned as a detached background task at the turn boundary. The expensive
/// summarization LLM round-trip happens *here*, between turns, so the next turn
/// starts from an already-compacted history instead of stalling on it. The
/// synchronous check at the top of `reply()` stays as the fallback for when this
/// hasn't landed yet (a huge single turn, a fast follow-up message, or a failed
/// background task) — that is the "keep a synchronous fallback" phasing BR-12
/// calls for.
///
/// Race-safe against a live turn: the swap goes through
/// [`SessionManager::replace_conversation_preserving_tail`], so a message a
/// concurrent turn appended while the summarizer ran is carried over rather than
/// destroyed, and a snapshot whose basis was truncated or rewritten underneath
/// us is refused outright — in which case the result is discarded and the next
/// turn's synchronous fallback handles it. That check runs inside the rewrite's
/// own transaction, so unlike the message-count compare it replaces it has no
/// window at all.
///
/// This makes the write safe, not the concurrency rare. Callers should still
/// serialize eager compactions per session (only one in flight) so two
/// summarizers don't both pay for a round-trip one of them will discard —
/// and note that `Agent`-scoped serialization cannot cover a second process on
/// the same `sessions.db`.
///
/// [`SessionManager::replace_conversation_preserving_tail`]: crate::session::SessionManager::replace_conversation_preserving_tail
/// `on_before_compact` runs exactly once, only when compaction is actually about
/// to happen (the threshold check passed) and just before the summarization call.
/// The caller uses it to fire the `PreCompact` hook so — unlike firing it around
/// every turn boundary — it never fires on a turn that ends under budget.
pub async fn run_eager_compaction(
    provider: Arc<dyn Provider>,
    session_manager: Arc<crate::session::SessionManager>,
    session_config: crate::agents::types::SessionConfig,
    threshold: f64,
    on_before_compact: impl FnOnce(),
) -> Result<EagerCompactionOutcome> {
    let (session, basis) = session_manager
        .snapshot_for_rewrite(&session_config.id)
        .await?;
    let Some(conversation) = session.conversation.clone() else {
        return Ok(EagerCompactionOutcome::NotNeeded);
    };

    // Re-check the threshold against fresh, provider-reported usage. On the happy
    // path `session.total_tokens` was just written by the completed turn, so no
    // cold-path system/tool estimate is needed (pass `None`).
    if !check_if_compaction_needed(
        provider.as_ref(),
        &conversation,
        Some(threshold),
        &session,
        None,
    )
    .await?
    {
        return Ok(EagerCompactionOutcome::NotNeeded);
    }

    on_before_compact();
    let (compacted, usage) = compact_messages(provider.as_ref(), &conversation, false).await?;

    // Swap under the store's own freshness guard: anything a concurrent turn
    // appended while we summarized is carried over instead of being clobbered,
    // and a snapshot whose basis moved is refused.
    let (outcome, _stored) = session_manager
        .replace_conversation_preserving_tail(&session_config.id, &compacted, basis, &conversation)
        .await?;
    if !outcome.stored() {
        debug!(
            "BR-12: eager compaction aborted for session {} ({:?}); the history \
             was rewritten under it",
            session_config.id, outcome
        );
        return Ok(EagerCompactionOutcome::Aborted);
    }

    let usage_event_key = uuid::Uuid::new_v4().to_string();
    crate::agents::reply_parts::apply_session_metrics(
        &session_manager,
        &session_config,
        &usage,
        true,
        &usage_event_key,
        Some(provider.get_name().to_string()),
    )
    .await?;

    Ok(EagerCompactionOutcome::Swapped)
}

fn filter_tool_responses<'a>(messages: &[&'a Message], remove_percent: u32) -> Vec<&'a Message> {
    fn has_tool_response(msg: &Message) -> bool {
        msg.content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolResponse(_)))
    }

    if remove_percent == 0 {
        return messages.to_vec();
    }

    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| has_tool_response(msg))
        .map(|(i, _)| i)
        .collect();

    if tool_indices.is_empty() {
        return messages.to_vec();
    }

    let num_to_remove = ((tool_indices.len() * remove_percent as usize) / 100).max(1);

    let middle = tool_indices.len() / 2;
    let mut indices_to_remove = Vec::new();

    // Middle out
    for i in 0..num_to_remove {
        if i % 2 == 0 {
            let offset = i / 2;
            if middle > offset {
                indices_to_remove.push(tool_indices[middle - offset - 1]);
            }
        } else {
            let offset = i / 2;
            if middle + offset < tool_indices.len() {
                indices_to_remove.push(tool_indices[middle + offset]);
            }
        }
    }

    messages
        .iter()
        .enumerate()
        .filter(|(i, _)| !indices_to_remove.contains(i))
        .map(|(_, msg)| *msg)
        .collect()
}

/// BR-11: head/tail ("middle-out") truncation of a single payload. Keeps the
/// first and last halves of `max_chars` and replaces the middle with a marker
/// recording how much was elided. Head+tail is deliberate — a single huge
/// message (a 400k-token paste or an un-removed oversized tool result) usually
/// carries its point at the top (what it is) and its most relevant lines at the
/// bottom (the tail of a log / the end of a file), both of which a plain
/// head-only or tail-only cut would throw away. Char-based, so it is safe on
/// multibyte text.
///
/// BR-6 reuses this helper to build the inline head/tail preview of an
/// offloaded oversized tool result, so compaction and the large-response
/// handler elide the middle the same way.
pub(crate) fn truncate_middle_out(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    // Reserve room for the elision marker so the result stays near the budget
    // even when `max_chars` is small.
    const MARKER_RESERVE: usize = 48;
    let keep = max_chars.saturating_sub(MARKER_RESERVE);
    let head_len = keep / 2;
    let tail_len = keep - head_len;
    let chars: Vec<char> = text.chars().collect();
    let head: String = chars[..head_len].iter().collect();
    let tail: String = chars[total - tail_len..].iter().collect();
    let elided = total - head_len - tail_len;
    format!("{head}\n…[{elided} characters elided]…\n{tail}")
}

/// BR-11: one attempt at building the summarizer input. The first attempts
/// preserve message fidelity and only drop whole tool responses middle-out; the
/// later attempts also head/tail-truncate the payload so a single over-window
/// message can no longer dead-end compaction (`per_msg_cap`/`total_cap` are
/// `None` on the fidelity attempts).
struct CompactionAttempt {
    remove_percent: u32,
    per_msg_cap: Option<usize>,
    total_cap: Option<usize>,
}

/// BR-14: the plain-text body of a (summary) message.
fn summary_text(msg: &Message) -> String {
    msg.content
        .iter()
        .filter_map(|c| match c {
            MessageContent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// BR-14: reject an empty or garbage summary before it becomes the memory the
/// strong model relies on. A usable summary is non-trivially long and names at
/// least a few of the mandated sections.
fn summary_is_usable(msg: &Message) -> bool {
    let text = summary_text(msg);
    let trimmed = text.trim();
    if trimmed.chars().count() < MIN_SUMMARY_CHARS {
        return false;
    }
    let lower = trimmed.to_lowercase();
    let present = MANDATED_SUMMARY_SECTIONS
        .iter()
        .filter(|section| lower.contains(&section.to_lowercase()))
        .count();
    present >= MIN_SUMMARY_SECTIONS
}

/// BR-14: run one summarization completion. Uses the main (session) model by
/// default so the strong model writes the memory it will later rely on, instead
/// of the weakest/fast model. The fast model is opt-in via
/// `BIOROUTER_COMPACT_USE_FAST_MODEL=true`, which restores the pre-BR-14
/// behaviour (cheaper, lower fidelity, and a possibly smaller context window
/// that overflows the summarizer more often).
async fn complete_summary(
    provider: &dyn Provider,
    use_fast_model: bool,
    system_prompt: &str,
    request: &[Message],
) -> Result<(Message, ProviderUsage), ProviderError> {
    if use_fast_model {
        provider.complete_fast(system_prompt, request, &[]).await
    } else {
        provider.complete(system_prompt, request, &[]).await
    }
}

/// BR-14: summarize with validation and a single retry. A junk/empty summary is
/// re-requested once; if the retry is still not well-formed we keep whichever
/// result is non-empty (a mediocre summary beats dead-ending the turn) and only
/// error when both are truly empty. A provider error on the *first* call is
/// propagated so [`do_compact`] can fall through to a smaller payload; a
/// provider error on the *retry* falls back to the non-empty first result.
async fn summarize_with_validation(
    provider: &dyn Provider,
    use_fast_model: bool,
    system_prompt: &str,
    request: &[Message],
) -> Result<(Message, ProviderUsage), ProviderError> {
    let (response, usage) =
        complete_summary(provider, use_fast_model, system_prompt, request).await?;
    if summary_is_usable(&response) {
        return Ok((response, usage));
    }

    warn!("Compaction summary failed validation (empty or missing sections); retrying once");
    match complete_summary(provider, use_fast_model, system_prompt, request).await {
        Ok((retry_response, retry_usage)) if summary_is_usable(&retry_response) => {
            Ok((retry_response, retry_usage))
        }
        Ok((retry_response, retry_usage)) => {
            if !summary_text(&retry_response).trim().is_empty() {
                Ok((retry_response, retry_usage))
            } else if !summary_text(&response).trim().is_empty() {
                Ok((response, usage))
            } else {
                Err(ProviderError::ExecutionError(
                    "Compaction produced an empty summary even after one retry".to_string(),
                ))
            }
        }
        Err(retry_err) => {
            if summary_text(&response).trim().is_empty() {
                Err(retry_err)
            } else {
                Ok((response, usage))
            }
        }
    }
}

async fn do_compact(
    provider: &dyn Provider,
    messages: &[Message],
) -> Result<(Message, ProviderUsage), anyhow::Error> {
    let agent_visible_messages: Vec<&Message> = messages
        .iter()
        .filter(|msg| msg.is_agent_visible())
        .collect();

    // BR-14: summarize with the main (session) model by default so the strong
    // model — not the weakest/fast one — writes the memory it later relies on.
    // Opt back into the cheaper fast-model summary with
    // `BIOROUTER_COMPACT_USE_FAST_MODEL=true` (trades fidelity for cost).
    let use_fast_model = Config::global()
        .get_param::<bool>("BIOROUTER_COMPACT_USE_FAST_MODEL")
        .unwrap_or(false);

    // BR-11: character budget for the summarizer payload, derived from the
    // model's token window. Kept to roughly half the window so the
    // summarize_oneshot prompt and the model's own reasoning still fit.
    let payload_budget_chars = provider
        .get_model_config()
        .context_limit()
        .saturating_mul(APPROX_CHARS_PER_TOKEN)
        / 2;

    // Try progressively cheaper inputs, stopping at the first that fits. The
    // first five preserve message fidelity and only drop whole tool responses
    // middle-out (`filter_tool_responses`). If a single message still overflows
    // the window (BR-11), the last three head/tail-truncate the payload — first
    // per message, finally with a hard cap on the whole thing — so an oversized
    // paste or tool result no longer dead-ends the session with "start a new
    // session".
    let attempts = [
        CompactionAttempt {
            remove_percent: 0,
            per_msg_cap: None,
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 10,
            per_msg_cap: None,
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 20,
            per_msg_cap: None,
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 50,
            per_msg_cap: None,
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 100,
            per_msg_cap: None,
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 100,
            per_msg_cap: Some(payload_budget_chars),
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 100,
            per_msg_cap: Some(payload_budget_chars / 2),
            total_cap: None,
        },
        CompactionAttempt {
            remove_percent: 100,
            per_msg_cap: Some(payload_budget_chars / 4),
            total_cap: Some(payload_budget_chars),
        },
    ];

    for (attempt, plan) in attempts.iter().enumerate() {
        let filtered_messages = filter_tool_responses(&agent_visible_messages, plan.remove_percent);

        let mut messages_text = filtered_messages
            .iter()
            .map(|&msg| {
                let formatted = format_message_for_compacting(msg);
                match plan.per_msg_cap {
                    Some(cap) => truncate_middle_out(&formatted, cap),
                    None => formatted,
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(total_cap) = plan.total_cap {
            messages_text = truncate_middle_out(&messages_text, total_cap);
        }

        let context = SummarizeContext {
            messages: messages_text,
        };

        let system_prompt = render_global_file("summarize_oneshot.md", &context)?;

        let user_message = Message::user()
            .with_text("Please summarize the conversation history provided in the system prompt.");
        let summarization_request = vec![user_message];

        match summarize_with_validation(
            provider,
            use_fast_model,
            &system_prompt,
            &summarization_request,
        )
        .await
        {
            Ok((mut response, mut provider_usage)) => {
                // BR-14: keep the summary as a User-role message. After compaction
                // it is the *first* agent-visible message, and `fix_conversation`'s
                // `fix_lead_trail` strips a leading *assistant* message — so
                // labelling the summary assistant would delete it and defeat
                // compaction. Providers that require the first turn to be `user`
                // (e.g. Anthropic) need this too. This is a deliberate framing of
                // recovered context as user input, not an accidental mislabel.
                response.role = Role::User;

                provider_usage
                    .ensure_tokens(&system_prompt, &summarization_request, &response, &[])
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to ensure usage tokens: {}", e))?;

                return Ok((response, provider_usage));
            }
            Err(e) => {
                if matches!(e, ProviderError::ContextLengthExceeded(_)) {
                    if attempt < attempts.len() - 1 {
                        continue;
                    } else {
                        return Err(anyhow::anyhow!(
                            "Failed to compact: context limit exceeded even after removing all tool responses and truncating the largest messages"
                        ));
                    }
                }
                return Err(e.into());
            }
        }
    }

    Err(anyhow::anyhow!(
        "Unexpected: exhausted all attempts without returning"
    ))
}

fn format_message_for_compacting(msg: &Message) -> String {
    let content_parts: Vec<String> = msg
        .content
        .iter()
        .map(|content| match content {
            MessageContent::Text(text) => text.text.clone(),
            MessageContent::Image(img) => format!("[image: {}]", img.mime_type),
            MessageContent::ToolRequest(req) => {
                if let Ok(call) = &req.tool_call {
                    format!(
                        "tool_request({}): {}",
                        call.name,
                        serde_json::to_string_pretty(&call.arguments)
                            .unwrap_or_else(|_| "<<invalid json>>".to_string())
                    )
                } else {
                    "tool_request: [error]".to_string()
                }
            }
            MessageContent::ToolResponse(res) => {
                if let Ok(result) = &res.tool_result {
                    let text_items: Vec<String> = result
                        .content
                        .iter()
                        .filter_map(|content| {
                            content.as_text().map(|text_str| text_str.text.clone())
                        })
                        .collect();

                    if !text_items.is_empty() {
                        format!("tool_response: {}", text_items.join("\n"))
                    } else {
                        "tool_response: [non-text content]".to_string()
                    }
                } else {
                    "tool_response: [error]".to_string()
                }
            }
            MessageContent::ToolConfirmationRequest(req) => {
                format!("tool_confirmation_request: {}", req.tool_name)
            }
            MessageContent::ActionRequired(action) => match &action.data {
                ActionRequiredData::ToolConfirmation { tool_name, .. } => {
                    format!("action_required(tool_confirmation): {}", tool_name)
                }
                ActionRequiredData::Elicitation { message, .. } => {
                    format!("action_required(elicitation): {}", message)
                }
                ActionRequiredData::ElicitationResponse { id, .. } => {
                    format!("action_required(elicitation_response): {}", id)
                }
            },
            MessageContent::FrontendToolRequest(req) => {
                if let Ok(call) = &req.tool_call {
                    format!("frontend_tool_request: {}", call.name)
                } else {
                    "frontend_tool_request: [error]".to_string()
                }
            }
            MessageContent::Thinking(thinking) => format!("thinking: {}", thinking.thinking),
            MessageContent::RedactedThinking(_) => "redacted_thinking".to_string(),
            MessageContent::SystemNotification(notification) => {
                format!("system_notification: {}", notification.msg)
            }
        })
        .collect();

    let role_str = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    if content_parts.is_empty() {
        format!("[{}]: <empty message>", role_str)
    } else {
        format!("[{}]: {}", role_str, content_parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::ModelConfig,
        providers::{
            base::{ProviderMetadata, Usage},
            errors::ProviderError,
        },
    };
    use async_trait::async_trait;
    use rmcp::model::{AnnotateAble, CallToolRequestParams, RawContent, Tool};

    pub(super) struct MockProvider {
        message: Message,
        config: ModelConfig,
        max_tool_responses: Option<usize>,
        max_input_chars: Option<usize>,
        // BR-14: when set, the first completion returns this (e.g. a garbage
        // summary) and later completions return `message`, so a validation
        // retry can be exercised.
        first_call_response: Option<Message>,
        call_count: std::sync::atomic::AtomicUsize,
        /// The summarizer carries the history it is asked to condense in the
        /// SYSTEM prompt, so recording it is how a test can assert on what did
        /// (and did not) reach the summarizer.
        last_system_prompt: std::sync::Mutex<String>,
    }

    impl MockProvider {
        pub(super) fn new(message: Message, context_limit: usize) -> Self {
            Self {
                message,
                config: ModelConfig {
                    model_name: "test".to_string(),
                    context_limit: Some(context_limit),
                    temperature: None,
                    max_tokens: None,
                    toolshim: false,
                    toolshim_model: None,
                    fast_model: None,
                    request_params: None,
                    reasoning_effort: None,
                },
                max_tool_responses: None,
                max_input_chars: None,
                first_call_response: None,
                call_count: std::sync::atomic::AtomicUsize::new(0),
                last_system_prompt: std::sync::Mutex::new(String::new()),
            }
        }

        /// The history the most recent summarization call was handed.
        pub(super) fn last_summarizer_payload(&self) -> String {
            self.last_system_prompt.lock().unwrap().clone()
        }

        fn with_max_tool_responses(mut self, max: usize) -> Self {
            self.max_tool_responses = Some(max);
            self
        }

        /// BR-14: return `first` on the first completion (e.g. an empty/garbage
        /// summary) and `message` thereafter, so a validation retry is exercised.
        fn with_first_response(mut self, first: Message) -> Self {
            self.first_call_response = Some(first);
            self
        }

        /// BR-14: how many completions have been requested so far.
        pub(super) fn calls(&self) -> usize {
            self.call_count.load(std::sync::atomic::Ordering::SeqCst)
        }

        /// Fail with `ContextLengthExceeded` when the summarizer's system prompt
        /// (which carries the formatted history) exceeds `max` characters —
        /// models a payload that is too large regardless of tool-response count,
        /// so only BR-11 truncation can recover.
        fn with_max_input_chars(mut self, max: usize) -> Self {
            self.max_input_chars = Some(max);
            self
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new("mock", "", "", "", vec![""], "", vec![])
        }

        fn get_name(&self) -> &str {
            "mock"
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            system: &str,
            messages: &[Message],
            _tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            let call_index = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            *self.last_system_prompt.lock().unwrap() = system.to_string();

            // If max_input_chars is set, fail when the payload (carried in the
            // summarizer system prompt) is too large.
            if let Some(max) = self.max_input_chars {
                if system.chars().count() > max {
                    return Err(ProviderError::ContextLengthExceeded(format!(
                        "Input too large: {} > {}",
                        system.chars().count(),
                        max
                    )));
                }
            }

            // If max_tool_responses is set, fail if we have too many
            if let Some(max) = self.max_tool_responses {
                let tool_response_count = messages
                    .iter()
                    .filter(|m| {
                        m.content
                            .iter()
                            .any(|c| matches!(c, MessageContent::ToolResponse(_)))
                    })
                    .count();

                if tool_response_count > max {
                    return Err(ProviderError::ContextLengthExceeded(format!(
                        "Too many tool responses: {} > {}",
                        tool_response_count, max
                    )));
                }
            }

            let response = if call_index == 0 {
                self.first_call_response
                    .clone()
                    .unwrap_or_else(|| self.message.clone())
            } else {
                self.message.clone()
            };

            Ok((
                response,
                ProviderUsage::new("mock-model".to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            self.config.clone()
        }
    }

    #[tokio::test]
    async fn test_keeps_tool_request() {
        let response_message = Message::assistant().with_text("<mock summary>");
        let provider = MockProvider::new(response_message, 1);
        let basic_conversation = vec![
            Message::user().with_text("read hello.txt"),
            Message::assistant().with_tool_request(
                "tool_0",
                Ok(CallToolRequestParams {
                    task: None,
                    name: "read_file".into(),
                    arguments: None,
                    meta: None,
                }),
            ),
            Message::user().with_tool_response(
                "tool_0",
                Ok(rmcp::model::CallToolResult {
                    content: vec![RawContent::text("hello, world").no_annotation()],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
            ),
        ];

        let conversation = Conversation::new_unvalidated(basic_conversation);
        let (compacted_conversation, _usage) = compact_messages(&provider, &conversation, false)
            .await
            .unwrap();

        let agent_conversation = compacted_conversation.agent_visible_messages();

        let _ = Conversation::new(agent_conversation)
            .expect("compaction should produce a valid conversation");
    }

    #[tokio::test]
    async fn test_progressive_removal_on_context_exceeded() {
        let response_message = Message::assistant().with_text("<mock summary>");
        // Set max to 2 tool responses - will trigger progressive removal
        let provider = MockProvider::new(response_message, 1000).with_max_tool_responses(2);

        // Create a conversation with many tool responses
        let mut messages = vec![Message::user().with_text("start")];
        for i in 0..10 {
            messages.push(Message::assistant().with_tool_request(
                format!("tool_{}", i),
                Ok(CallToolRequestParams {
                    task: None,
                    name: "read_file".into(),
                    arguments: None,
                    meta: None,
                }),
            ));
            messages.push(Message::user().with_tool_response(
                format!("tool_{}", i),
                Ok(rmcp::model::CallToolResult {
                    content: vec![RawContent::text(format!("response{}", i)).no_annotation()],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
            ));
        }

        let conversation = Conversation::new_unvalidated(messages);
        let result = compact_messages(&provider, &conversation, false).await;

        // Should succeed after progressive removal
        assert!(
            result.is_ok(),
            "Should succeed with progressive removal: {:?}",
            result.err()
        );
    }

    /// BR-15: the cold-path estimate (no provider-reported usage) must fold in
    /// the system prompt and tool schemas. A conversation that is comfortably
    /// under the window on messages alone must trip the threshold once the
    /// system prompt + tools are included.
    #[tokio::test]
    async fn test_cold_path_estimate_includes_system_and_tools() {
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 1000);
        // No provider-reported usage → the fallback estimate runs.
        let session = crate::session::Session {
            total_tokens: None,
            ..Default::default()
        };
        let conversation =
            Conversation::new_unvalidated(vec![Message::user().with_text("hello there")]);

        // Messages-only estimate: a two-word user turn is far under 80% of 1000.
        let without =
            check_if_compaction_needed(&provider, &conversation, Some(0.8), &session, None)
                .await
                .unwrap();
        assert!(!without, "tiny message alone should not trip the threshold");

        // A large system prompt plus a tool pushes the same conversation over.
        let big_system = "word ".repeat(1200);
        let tool = Tool::new(
            "big_tool".to_string(),
            "A tool with a long description ".repeat(20),
            std::sync::Arc::new(serde_json::Map::new()),
        );
        let with = check_if_compaction_needed(
            &provider,
            &conversation,
            Some(0.8),
            &session,
            Some((big_system.as_str(), std::slice::from_ref(&tool))),
        )
        .await
        .unwrap();
        assert!(
            with,
            "including the system prompt + tool schemas should trip the threshold"
        );
    }

    /// BR-10: the split point snaps to genuine user-prompt boundaries and only
    /// applies when there are strictly more turns than we keep.
    #[test]
    fn test_recent_window_split_snaps_to_user_prompt() {
        let msgs = vec![
            Message::user().with_text("q1"),
            Message::assistant().with_text("a1"),
            Message::user().with_text("q2"),
            Message::assistant().with_text("a2"),
            Message::user().with_text("q3"),
            Message::assistant().with_text("a3"),
        ];
        // q1@0, q2@2, q3@4 are the three boundaries.
        assert_eq!(recent_window_split(&msgs, 1), Some(4));
        assert_eq!(recent_window_split(&msgs, 2), Some(2));
        // Keeping >= all turns leaves no older prefix to summarize.
        assert_eq!(recent_window_split(&msgs, 3), None);
        assert_eq!(recent_window_split(&msgs, 4), None);
        // Disabled.
        assert_eq!(recent_window_split(&msgs, 0), None);
    }

    /// BR-10: a tool-response user message must not count as a turn boundary, so
    /// the cut never lands between a tool_request and its tool_response.
    #[test]
    fn test_recent_window_split_ignores_tool_responses() {
        let msgs = vec![
            Message::user().with_text("q1"),
            Message::assistant().with_tool_request(
                "tool_0",
                Ok(CallToolRequestParams {
                    task: None,
                    name: "read_file".into(),
                    arguments: None,
                    meta: None,
                }),
            ),
            Message::user().with_tool_response(
                "tool_0",
                Ok(rmcp::model::CallToolResult {
                    content: vec![RawContent::text("f").no_annotation()],
                    structured_content: None,
                    is_error: Some(false),
                    meta: None,
                }),
            ),
            Message::assistant().with_text("a1"),
            Message::user().with_text("q2"),
            Message::assistant().with_text("a2"),
        ];
        // Only q1@0 and q2@4 are boundaries; the tool_response@2 is not.
        assert_eq!(recent_window_split(&msgs, 1), Some(4));
    }

    /// BR-10: the most recent N turns stay verbatim (agent-visible) while only
    /// the older prefix is summarized and flipped agent-invisible.
    #[tokio::test]
    async fn test_recent_window_keeps_recent_turns_verbatim() {
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 100_000);

        let mut messages = Vec::new();
        for i in 1..=4 {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        // Turn 5 carries a tool_request/tool_response pair.
        messages.push(Message::user().with_text("q5"));
        messages.push(Message::assistant().with_tool_request(
            "tool_0",
            Ok(CallToolRequestParams {
                task: None,
                name: "read_file".into(),
                arguments: None,
                meta: None,
            }),
        ));
        messages.push(Message::user().with_tool_response(
            "tool_0",
            Ok(rmcp::model::CallToolResult {
                content: vec![RawContent::text("f").no_annotation()],
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
        ));
        messages.push(Message::assistant().with_text("a5"));
        // Turn 6.
        messages.push(Message::user().with_text("q6"));
        messages.push(Message::assistant().with_text("a6"));

        let conversation = Conversation::new_unvalidated(messages);
        // Keep the last 2 turns verbatim; turns 1-4 become the summarized prefix.
        let (compacted, _usage) = compact_messages_with_window(&provider, &conversation, false, 2)
            .await
            .unwrap();

        let text_of = |m: &Message| -> String {
            m.content
                .iter()
                .filter_map(|c| {
                    if let MessageContent::Text(t) = c {
                        Some(t.text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        };

        // Older turns are retained in the transcript but hidden from the agent.
        for label in ["q1", "q2", "q3", "q4", "a1", "a4"] {
            let msg = compacted
                .messages()
                .iter()
                .find(|m| text_of(m) == label)
                .unwrap_or_else(|| panic!("{label} missing from transcript"));
            assert!(
                !msg.is_agent_visible(),
                "{label} should be agent-invisible after compaction"
            );
        }

        // The prefix summary is agent-visible.
        let summary = compacted
            .messages()
            .iter()
            .find(|m| text_of(m) == "<mock summary>")
            .expect("summary present");
        assert!(summary.is_agent_visible());

        // The recent turns are kept verbatim and stay agent-visible.
        for label in ["q5", "q6", "a5", "a6"] {
            let msg = compacted
                .messages()
                .iter()
                .find(|m| text_of(m) == label)
                .unwrap_or_else(|| panic!("{label} missing"));
            assert!(
                msg.is_agent_visible(),
                "{label} should be kept verbatim (agent-visible)"
            );
        }

        // The kept tool_request/tool_response pair both survive, agent-visible.
        let agent_msgs = compacted.agent_visible_messages();
        let requests = agent_msgs
            .iter()
            .filter(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, MessageContent::ToolRequest(_)))
            })
            .count();
        let responses = agent_msgs
            .iter()
            .filter(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, MessageContent::ToolResponse(_)))
            })
            .count();
        assert_eq!(requests, 1, "kept tool_request should survive");
        assert_eq!(responses, 1, "kept tool_response should survive");
    }

    /// BR-11: `truncate_middle_out` keeps the head and tail and elides the
    /// middle, leaves short text untouched, and never panics on tiny budgets.
    #[test]
    fn test_truncate_middle_out_keeps_head_and_tail() {
        let text: String = "abcdefghij".repeat(1000); // 10_000 ASCII chars
        let out = truncate_middle_out(&text, 1000);
        assert!(
            out.chars().count() < text.chars().count(),
            "over-budget text should shrink"
        );
        assert!(
            out.contains("characters elided"),
            "an elision marker should be present"
        );
        let head: String = text.chars().take(100).collect();
        assert!(out.starts_with(&head), "head should be preserved");
        let tail: String = text.chars().skip(text.chars().count() - 100).collect();
        assert!(out.ends_with(&tail), "tail should be preserved");

        // Under-budget text is returned verbatim.
        assert_eq!(truncate_middle_out("small", 1000), "small");
        // A tiny budget must not panic (head/tail collapse to the marker).
        let tiny = truncate_middle_out(&text, 4);
        assert!(tiny.contains("characters elided"));
    }

    /// BR-11: a single message that alone overflows the window — with no tool
    /// responses to drop — is no longer a dead end. Head/tail truncation of the
    /// payload lets compaction succeed instead of erroring out and telling the
    /// user to start a new session.
    #[tokio::test]
    async fn test_truncates_single_oversized_message() {
        // context_limit 1000 -> payload budget = 1000 * 4 / 2 = 2000 chars.
        // Fail whenever the summarizer system prompt carries > 5000 chars, so
        // the fidelity attempts (payload ~= 50_000) all fail and only the
        // truncation fallback fits.
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 1000)
            .with_max_input_chars(5000);

        let huge = "x".repeat(50_000);
        let conversation = Conversation::new_unvalidated(vec![Message::user().with_text(huge)]);

        // keep_last_turns = 0 forces the legacy (non-windowed) path so the whole
        // oversized message reaches do_compact.
        let (compacted, _usage) = compact_messages_with_window(&provider, &conversation, false, 0)
            .await
            .expect("BR-11 truncation should recover instead of dead-ending");

        let agent_msgs = compacted.agent_visible_messages();
        Conversation::new(agent_msgs).expect("compaction should produce a valid conversation");
    }

    /// BR-11: when the payload cannot fit even after full truncation, compaction
    /// still surfaces a clear error (rather than looping) — the truncation path
    /// is a recovery, not a guarantee against a pathologically tiny window.
    #[tokio::test]
    async fn test_truncation_still_errors_when_nothing_fits() {
        // max_input_chars smaller than the summarize prompt itself: no payload
        // can ever fit, so every attempt fails and do_compact returns an error.
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 1000)
            .with_max_input_chars(1);

        let conversation =
            Conversation::new_unvalidated(vec![Message::user().with_text("x".repeat(50_000))]);

        let result = compact_messages_with_window(&provider, &conversation, false, 0).await;
        assert!(result.is_err(), "an impossible window should error cleanly");
    }

    /// BR-10: with too few turns to keep a window, compaction falls back to the
    /// legacy summarize-everything behaviour (the latest user message survives).
    #[tokio::test]
    async fn test_recent_window_falls_back_when_too_few_turns() {
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 1);
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("read hello.txt"),
            Message::assistant().with_text("done"),
        ]);
        // keep_last_turns larger than the number of turns -> fallback path.
        let (compacted, _usage) = compact_messages_with_window(&provider, &conversation, false, 4)
            .await
            .unwrap();
        let agent_msgs = compacted.agent_visible_messages();
        Conversation::new(agent_msgs).expect("fallback compaction should validate");
    }

    /// BR-13: the reactive-overflow ladder escalates strictly and terminates —
    /// keep-window → shrink-window → summarize-all → drop-oldest → give up.
    #[test]
    fn test_overflow_recovery_ladder_escalates() {
        assert_eq!(
            overflow_recovery_for_attempt(1),
            Some(OverflowRecovery::Compact)
        );
        assert_eq!(
            overflow_recovery_for_attempt(2),
            Some(OverflowRecovery::ShrinkWindow(2))
        );
        assert_eq!(
            overflow_recovery_for_attempt(3),
            Some(OverflowRecovery::SummarizeAll)
        );
        assert_eq!(
            overflow_recovery_for_attempt(4),
            Some(OverflowRecovery::DropOldestThenSummarize)
        );
        // Ladder exhausted -> give up. This is the ONLY path that surfaces the
        // "context limit still exceeded" error to the user.
        assert_eq!(overflow_recovery_for_attempt(5), None);
        assert_eq!(overflow_recovery_for_attempt(99), None);
    }

    /// BR-13: dropping the oldest turns flips the older half agent-invisible while
    /// keeping the newer half, and the cut snaps to a user-prompt boundary.
    #[test]
    fn test_drop_oldest_agent_visible_turns_snaps_boundary() {
        let msgs: Vec<Message> = (1..=4)
            .flat_map(|i| {
                vec![
                    Message::user().with_text(format!("q{i}")),
                    Message::assistant().with_text(format!("a{i}")),
                ]
            })
            .collect();
        // boundaries q1@0, q2@2, q3@4, q4@6 -> cut = boundaries[4/2] = index 4 (q3).
        let out = drop_oldest_agent_visible_turns(&msgs);
        for (i, m) in out.iter().enumerate() {
            if i < 4 {
                assert!(!m.is_agent_visible(), "index {i} should be dropped");
            } else {
                assert!(m.is_agent_visible(), "index {i} should be kept");
            }
        }

        // With a single user prompt there is nothing safe to drop.
        let one = vec![
            Message::user().with_text("q1"),
            Message::assistant().with_text("a1"),
        ];
        let out = drop_oldest_agent_visible_turns(&one);
        assert!(
            out.iter().all(|m| m.is_agent_visible()),
            "too few turns to drop -> conversation left untouched"
        );
    }

    /// BR-13: escalating from a shrunken verbatim window to summarize-everything
    /// leaves strictly fewer agent-visible messages, which is exactly how the
    /// ladder claws a wedged session back under the window.
    #[tokio::test]
    async fn test_recovery_summarize_all_collapses_recent_window() {
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 100_000);
        let messages: Vec<Message> = (1..=8)
            .flat_map(|i| {
                vec![
                    Message::user().with_text(format!("q{i}")),
                    Message::assistant().with_text(format!("a{i}")),
                ]
            })
            .collect();
        let conversation = Conversation::new_unvalidated(messages);

        let (shrunk, _) = compact_messages_with_recovery(
            &provider,
            &conversation,
            OverflowRecovery::ShrinkWindow(2),
        )
        .await
        .unwrap();
        let (all, _) = compact_messages_with_recovery(
            &provider,
            &conversation,
            OverflowRecovery::SummarizeAll,
        )
        .await
        .unwrap();

        let visible = |c: &Conversation| c.agent_visible_messages().len();
        assert!(
            visible(&all) < visible(&shrunk),
            "summarize-all should leave fewer agent-visible messages than a shrunk window: all={}, shrunk={}",
            visible(&all),
            visible(&shrunk),
        );
    }

    /// BR-13: the last-resort drop-oldest stage produces a valid conversation
    /// (oldest turns dropped, remainder summarized) instead of dead-ending.
    #[tokio::test]
    async fn test_recovery_drop_oldest_then_summarize() {
        let provider = MockProvider::new(Message::assistant().with_text("<mock summary>"), 100_000);
        let messages: Vec<Message> = (1..=6)
            .flat_map(|i| {
                vec![
                    Message::user().with_text(format!("q{i}")),
                    Message::assistant().with_text(format!("a{i}")),
                ]
            })
            .collect();
        let conversation = Conversation::new_unvalidated(messages);

        let (compacted, _) = compact_messages_with_recovery(
            &provider,
            &conversation,
            OverflowRecovery::DropOldestThenSummarize,
        )
        .await
        .expect("drop-oldest recovery should succeed");

        let agent_msgs = compacted.agent_visible_messages();
        Conversation::new(agent_msgs)
            .expect("drop-oldest recovery should produce a valid conversation");
    }

    /// BR-14: the summary validator accepts a well-formed summary and rejects an
    /// empty one, a one-liner, and a refusal that names no mandated sections.
    #[test]
    fn test_summary_is_usable() {
        let good = Message::assistant().with_text(
            "User Intent: continue the task.\nTechnical Concepts: rust, tokio.\n\
             Files + Code: mod.rs.\nPending Tasks: finish BR-14.",
        );
        assert!(summary_is_usable(&good), "a well-formed summary is usable");

        // Empty / whitespace-only.
        assert!(!summary_is_usable(&Message::assistant().with_text("   ")));
        // Non-empty but far too short and section-less.
        assert!(!summary_is_usable(&Message::assistant().with_text("ok")));
        // A refusal: long enough, but names none of the mandated sections.
        let refusal = Message::assistant()
            .with_text("I'm sorry, but I can't help summarize this conversation right now.");
        assert!(!summary_is_usable(&refusal), "a refusal is not a summary");
        // Non-text content alone is not a usable summary.
        assert!(!summary_is_usable(&Message::assistant()));
    }

    /// BR-14: a garbage first summary is retried once, and the valid retry — not
    /// the junk — is what lands in the compacted conversation.
    #[tokio::test]
    async fn test_compaction_retries_on_garbage_summary() {
        let good = Message::assistant().with_text(
            "User Intent: continue the task.\nTechnical Concepts: rust, tokio.\n\
             Files + Code: mod.rs.\nPending Tasks: finish BR-14.",
        );
        let provider = MockProvider::new(good, 100_000)
            .with_first_response(Message::assistant().with_text("no"));

        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("q1"),
            Message::assistant().with_text("a1"),
        ]);
        // keep_last_turns = 0 forces the legacy path so do_compact runs directly.
        let (compacted, _usage) = compact_messages_with_window(&provider, &conversation, false, 0)
            .await
            .unwrap();

        let has_valid_summary = compacted.messages().iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, MessageContent::Text(t) if t.text.contains("User Intent")))
        });
        assert!(
            has_valid_summary,
            "the retried, valid summary should be used, not the garbage first response"
        );
        assert!(
            !compacted.messages().iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, MessageContent::Text(t) if t.text.trim() == "no"))
            }),
            "the garbage first response must not survive"
        );
        assert_eq!(
            provider.calls(),
            2,
            "compaction should make exactly one retry on a garbage summary"
        );
    }

    /// BR-14: the summary lands as a `Role::User` message. This is load-bearing —
    /// it is the first agent-visible message after compaction and
    /// `fix_conversation`'s `fix_lead_trail` deletes a leading *assistant*
    /// message, which would drop the summary and defeat compaction.
    #[tokio::test]
    async fn test_summary_lands_as_user_role() {
        let summary = Message::assistant()
            .with_text("User Intent: keep going.\nTechnical Concepts: none.\nPending Tasks: none.");
        let provider = MockProvider::new(summary, 100_000);
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("q1"),
            Message::assistant().with_text("a1"),
        ]);

        let (compacted, _usage) = compact_messages_with_window(&provider, &conversation, false, 0)
            .await
            .unwrap();

        let summary_msg = compacted
            .messages()
            .iter()
            .find(|m| {
                m.is_agent_visible()
                    && m.content.iter().any(
                        |c| matches!(c, MessageContent::Text(t) if t.text.contains("User Intent")),
                    )
            })
            .expect("summary present and agent-visible");
        assert_eq!(
            summary_msg.role,
            Role::User,
            "summary must be a User-role message so fix_lead_trail keeps it"
        );
    }

    // ---- BR-12: eager (background, between-turns) compaction ----

    /// Build a `SessionManager` backed by a throwaway temp dir + a session
    /// preloaded with `messages`, its live gauge set to `total_tokens`.
    async fn eager_test_session(
        messages: Vec<Message>,
        total_tokens: i32,
    ) -> (
        tempfile::TempDir,
        Arc<crate::session::SessionManager>,
        String,
    ) {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        let session = manager
            .create_session(
                std::path::PathBuf::from("/tmp"),
                "eager-test".to_string(),
                crate::session::SessionType::User,
            )
            .await
            .unwrap();
        for m in &messages {
            manager.add_message(&session.id, m).await.unwrap();
        }
        manager
            .update(&session.id)
            .total_tokens(Some(total_tokens))
            .apply()
            .await
            .unwrap();
        (temp, manager, session.id)
    }

    fn eager_session_config(id: &str) -> crate::agents::types::SessionConfig {
        crate::agents::types::SessionConfig {
            id: id.to_string(),
            schedule_id: None,
            max_turns: None,
            max_tool_calls: None,
            budget: None,
            retry_config: None,
            reasoning_effort: None,
        }
    }

    /// A multi-turn history that ends over budget is compacted and swapped in by
    /// the background routine, off the reply() critical path.
    #[tokio::test]
    async fn test_run_eager_compaction_swaps_over_threshold() {
        // Six plain user-prompt turns so the recent-window path (keep_last_turns
        // default 4) has an older prefix to summarize.
        let mut messages = Vec::new();
        for i in 0..6 {
            messages.push(Message::user().with_text(format!("question {i}")));
            messages.push(Message::assistant().with_text(format!("answer {i}")));
        }
        let (_temp, manager, id) = eager_test_session(messages, 90).await;

        // context_limit 100, total_tokens 90 -> 0.9 ratio, over the 0.8 threshold.
        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new(
            Message::assistant().with_text("<mock summary>"),
            100,
        ));

        let before_compact = std::sync::atomic::AtomicBool::new(false);
        let outcome = run_eager_compaction(
            provider,
            Arc::clone(&manager),
            eager_session_config(&id),
            0.8,
            || before_compact.store(true, std::sync::atomic::Ordering::SeqCst),
        )
        .await
        .unwrap();
        assert_eq!(outcome, EagerCompactionOutcome::Swapped);
        assert!(
            before_compact.load(std::sync::atomic::Ordering::SeqCst),
            "on_before_compact (PreCompact) must fire when compaction proceeds"
        );

        let reloaded = manager
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert!(
            reloaded.messages().iter().any(|m| !m.is_agent_visible()),
            "the summarized prefix should be flipped agent-invisible"
        );
        assert!(
            reloaded.messages().iter().any(|m| m.content.iter().any(
                |c| matches!(c, MessageContent::Text(t) if t.text.contains("<mock summary>"))
            )),
            "the compacted history should carry the summary"
        );
    }

    /// A provider that mutates the session's history from inside its own
    /// completion call — the deterministic stand-in for a concurrent writer
    /// landing in the summarizer's window. No sleeps, no barriers.
    struct MutatingProvider {
        inner: MockProvider,
        manager: Arc<crate::session::SessionManager>,
        session_id: String,
        mutation: Mutation,
    }

    enum Mutation {
        /// Another writer appends a message (BR-71's note, `term log`, a turn).
        Append(String),
        /// A checkpoint restore / message edit removes the basis' tail.
        Truncate(i64),
    }

    #[async_trait]
    impl Provider for MutatingProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new("mock", "", "", "", vec![""], "", vec![])
        }
        fn get_name(&self) -> &str {
            "mock"
        }
        async fn complete_with_model(
            &self,
            model_config: &ModelConfig,
            system: &str,
            messages: &[Message],
            tools: &[Tool],
        ) -> Result<(Message, ProviderUsage), ProviderError> {
            match &self.mutation {
                Mutation::Append(text) => {
                    self.manager
                        .add_message(&self.session_id, &Message::user().with_text(text.clone()))
                        .await
                        .unwrap();
                }
                Mutation::Truncate(ts) => {
                    self.manager
                        .truncate_conversation(&self.session_id, *ts)
                        .await
                        .unwrap();
                }
            }
            self.inner
                .complete_with_model(model_config, system, messages, tools)
                .await
        }
        fn get_model_config(&self) -> ModelConfig {
            self.inner.get_model_config()
        }
    }

    /// A well-formed summary, so `summary_is_usable` accepts it first time.
    fn mock_summary() -> Message {
        Message::assistant().with_text(
            "## User Intent\nplot the data\n## Technical Concepts\nplotting\n\
             ## Files\nnone\n## Pending Tasks\nnone\n",
        )
    }

    fn eager_texts(conversation: &Conversation) -> Vec<String> {
        conversation
            .messages()
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// A message appended while the background summarizer ran must survive AND
    /// the compaction must still land. The message-count guard this replaces
    /// discarded the compaction outright, so the session kept re-paying for a
    /// summarization it threw away.
    #[tokio::test]
    async fn run_eager_compaction_preserves_a_concurrent_append() {
        let mut messages = Vec::new();
        for i in 0..6 {
            messages.push(Message::user().with_text(format!("question {i}")));
            messages.push(Message::assistant().with_text(format!("answer {i}")));
        }
        let (_temp, manager, id) = eager_test_session(messages, 90).await;

        let provider: Arc<dyn Provider> = Arc::new(MutatingProvider {
            inner: MockProvider::new(mock_summary(), 100),
            manager: Arc::clone(&manager),
            session_id: id.clone(),
            mutation: Mutation::Append("NOTE: from a concurrent writer".to_string()),
        });

        let outcome = run_eager_compaction(
            provider,
            Arc::clone(&manager),
            eager_session_config(&id),
            0.8,
            || {},
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            EagerCompactionOutcome::Swapped,
            "the compaction must land, not be discarded"
        );
        let texts = eager_texts(
            &manager
                .get_session(&id, true)
                .await
                .unwrap()
                .conversation
                .unwrap(),
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("NOTE: from a concurrent writer")),
            "the concurrently appended note must survive; stored: {texts:#?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("User Intent")),
            "the summary must be persisted; stored: {texts:#?}"
        );
    }

    /// When the basis itself is truncated underneath the summarizer there is no
    /// sound prefix to merge onto: abort, write nothing, and let the next turn's
    /// synchronous fallback re-derive. First integration coverage of `Aborted`.
    #[tokio::test]
    async fn run_eager_compaction_aborts_when_the_prefix_moved() {
        let mut messages = Vec::new();
        for i in 0..6 {
            messages.push(Message::user().with_text(format!("question {i}")));
            messages.push(Message::assistant().with_text(format!("answer {i}")));
        }
        let (_temp, manager, id) = eager_test_session(messages, 90).await;
        let before = manager.conversation_revision(&id).await.unwrap();

        let provider: Arc<dyn Provider> = Arc::new(MutatingProvider {
            inner: MockProvider::new(mock_summary(), 100),
            manager: Arc::clone(&manager),
            session_id: id.clone(),
            // Messages carry `created = 0` here, so this removes the lot.
            mutation: Mutation::Truncate(0),
        });

        let outcome = run_eager_compaction(
            provider,
            Arc::clone(&manager),
            eager_session_config(&id),
            0.8,
            || {},
        )
        .await
        .unwrap();

        assert_eq!(outcome, EagerCompactionOutcome::Aborted);
        let after = manager.conversation_revision(&id).await.unwrap();
        assert_ne!(before, after, "the truncate itself did happen");
        assert_eq!(
            after.message_count(),
            0,
            "an aborted swap must not resurrect the truncated messages"
        );
    }

    /// A session comfortably under budget is left untouched — no summarization
    /// call, no swap.
    #[tokio::test]
    async fn test_run_eager_compaction_noop_under_threshold() {
        let messages = vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("hello"),
        ];
        let (_temp, manager, id) = eager_test_session(messages, 10).await;

        let provider: Arc<dyn Provider> = Arc::new(MockProvider::new(
            Message::assistant().with_text("<mock summary>"),
            100,
        ));

        let before_compact = std::sync::atomic::AtomicBool::new(false);
        let outcome = run_eager_compaction(
            provider,
            Arc::clone(&manager),
            eager_session_config(&id),
            0.8,
            || before_compact.store(true, std::sync::atomic::Ordering::SeqCst),
        )
        .await
        .unwrap();
        assert_eq!(outcome, EagerCompactionOutcome::NotNeeded);
        assert!(
            !before_compact.load(std::sync::atomic::Ordering::SeqCst),
            "PreCompact must not fire when the session is under budget"
        );

        let reloaded = manager
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(
            reloaded.messages().len(),
            2,
            "an under-budget session must not be rewritten"
        );
        assert!(
            reloaded.messages().iter().all(|m| m.is_agent_visible()),
            "nothing should be hidden when compaction did not run"
        );
    }
}

/// #51 part (b): the preservation marker, honoured.
///
/// Part (a) stopped a concurrent append from being destroyed by a compaction's
/// write-back. That is necessary and not sufficient: the message still falls out
/// of the verbatim window a few turns later and is dissolved into a summary. The
/// tests below are the whole retention rule —
///
///   never dropped by SUMMARIZATION; still subject to the overall context
///   BUDGET, and when the budget bites the OLDEST pins go first, loudly.
///
/// — asserted once per compaction path, because a marker honoured by three of
/// five paths is worse than no marker at all.
#[cfg(test)]
mod pin_tests {
    use super::tests::MockProvider;
    use super::*;
    use crate::conversation::Conversation;
    use rmcp::model::AnnotateAble;

    const NOTE: &str = "NOTE: the reviewer wants a log scale";

    fn provider(context_limit: usize) -> MockProvider {
        MockProvider::new(
            Message::assistant().with_text("<mock summary>"),
            context_limit,
        )
    }

    fn text_of(m: &Message) -> String {
        m.content
            .iter()
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// `turns` plain user/assistant turns, with `note` spliced in as a pinned
    /// user message right after the FIRST turn — i.e. as far from the verbatim
    /// window as a message can get.
    fn conversation_with_an_old_pin(turns: usize) -> Conversation {
        let mut messages = vec![
            Message::user().with_text("q1"),
            Message::assistant().with_text("a1"),
            Message::user().with_text(NOTE).pinned(),
        ];
        for i in 2..=turns {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        Conversation::new_unvalidated(messages)
    }

    fn find<'a>(c: &'a Conversation, text: &str) -> &'a Message {
        c.messages()
            .iter()
            .find(|m| text_of(m) == text)
            .unwrap_or_else(|| panic!("{text:?} missing from the compacted transcript"))
    }

    fn agent_texts(c: &Conversation) -> Vec<String> {
        c.agent_visible_messages().iter().map(text_of).collect()
    }

    // ── P1: the headline case ───────────────────────────────────────────────

    /// A pinned message eight turns back must still reach the model after a
    /// normal compaction. Its unpinned neighbours in the same prefix must not:
    /// that control is what makes the assertion mean "the marker worked" rather
    /// than "nothing was compacted".
    #[tokio::test]
    async fn a_pinned_message_survives_a_normal_compaction() {
        let conversation = conversation_with_an_old_pin(8);
        let (compacted, _usage) =
            compact_messages_with_window(&provider(100_000), &conversation, false, 2)
                .await
                .unwrap();

        assert!(
            find(&compacted, NOTE).is_agent_visible(),
            "the pinned note must survive summarization; agent-visible was: {:#?}",
            agent_texts(&compacted)
        );
        assert!(
            find(&compacted, NOTE).is_pinned(),
            "and it must still be marked, so the NEXT compaction honours it too"
        );

        // Controls: its unpinned prefix neighbours were summarized away, and the
        // recent window is untouched.
        for hidden in ["q1", "a1", "q2", "a2"] {
            assert!(
                !find(&compacted, hidden).is_agent_visible(),
                "{hidden} is not pinned and should have been summarized away"
            );
        }
        for kept in ["q8", "a8"] {
            assert!(find(&compacted, kept).is_agent_visible());
        }
        assert!(find(&compacted, "<mock summary>").is_agent_visible());
    }

    /// The same guarantee for the hardest position: the pin is the very FIRST
    /// message of the conversation.
    #[tokio::test]
    async fn the_oldest_message_survives_when_it_is_pinned() {
        let mut messages = vec![Message::user().with_text(NOTE).pinned()];
        for i in 1..=8 {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        let conversation = Conversation::new_unvalidated(messages);

        let (compacted, _usage) =
            compact_messages_with_window(&provider(100_000), &conversation, false, 2)
                .await
                .unwrap();

        assert!(
            find(&compacted, NOTE).is_agent_visible(),
            "agent-visible was: {:#?}",
            agent_texts(&compacted)
        );
        assert!(!find(&compacted, "q1").is_agent_visible());
    }

    /// Compaction is idempotent over a pin: running it repeatedly must not
    /// erode the note. A pin that survives once and dies on the third pass is
    /// still a broken promise, just a slower one.
    #[tokio::test]
    async fn a_pin_survives_repeated_compactions() {
        let mut conversation = conversation_with_an_old_pin(8);
        for pass in 1..=3 {
            let (next, _usage) =
                compact_messages_with_window(&provider(100_000), &conversation, false, 2)
                    .await
                    .unwrap();
            assert!(
                find(&next, NOTE).is_agent_visible(),
                "pass {pass} dropped the pin; agent-visible was: {:#?}",
                agent_texts(&next)
            );
            conversation = next;
        }
    }

    // ── P2: every rung of the recovery ladder ───────────────────────────────

    /// `SummarizeAll` (`keep_last_turns == 0`) takes the legacy path, where
    /// EVERY message is flipped agent-invisible. The pin must be the exception.
    #[tokio::test]
    async fn a_pin_survives_the_summarize_all_rung() {
        let conversation = conversation_with_an_old_pin(8);
        let (compacted, _usage) = compact_messages_with_recovery(
            &provider(100_000),
            &conversation,
            OverflowRecovery::SummarizeAll,
        )
        .await
        .unwrap();

        assert!(
            find(&compacted, NOTE).is_agent_visible(),
            "agent-visible was: {:#?}",
            agent_texts(&compacted)
        );
        assert!(!find(&compacted, "q8").is_agent_visible(), "control");
    }

    /// `DropOldestThenSummarize` hard-drops the older half by flipping it
    /// agent-invisible BEFORE summarizing. That pre-pass is a second, separate
    /// place a pin can be erased.
    #[tokio::test]
    async fn a_pin_survives_the_drop_oldest_rung() {
        let conversation = conversation_with_an_old_pin(8);
        let (compacted, _usage) = compact_messages_with_recovery(
            &provider(100_000),
            &conversation,
            OverflowRecovery::DropOldestThenSummarize,
        )
        .await
        .unwrap();

        assert!(
            find(&compacted, NOTE).is_agent_visible(),
            "agent-visible was: {:#?}",
            agent_texts(&compacted)
        );
    }

    /// The whole ladder, in order, on one conversation — the shape a genuinely
    /// wedged session takes. The pin must be there at the bottom rung.
    #[tokio::test]
    async fn a_pin_survives_the_whole_recovery_ladder() {
        let mut conversation = conversation_with_an_old_pin(8);
        let mut attempt = 1;
        while let Some(recovery) = overflow_recovery_for_attempt(attempt) {
            let (next, _usage) =
                compact_messages_with_recovery(&provider(100_000), &conversation, recovery)
                    .await
                    .unwrap_or_else(|e| panic!("rung {recovery:?} failed: {e}"));
            assert!(
                find(&next, NOTE).is_agent_visible(),
                "rung {recovery:?} dropped the pin; agent-visible was: {:#?}",
                agent_texts(&next)
            );
            conversation = next;
            attempt += 1;
        }
        assert_eq!(attempt, 5, "the ladder should have run all four rungs");
    }

    /// `drop_oldest_agent_visible_turns` in isolation: the pre-pass must spare
    /// the pin while dropping its unpinned neighbours.
    #[test]
    fn drop_oldest_spares_a_pin() {
        let mut messages: Vec<Message> = Vec::new();
        for i in 1..=4 {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        messages.insert(1, Message::user().with_text(NOTE).pinned());

        let out = drop_oldest_agent_visible_turns(&messages);
        let pin = out.iter().find(|m| text_of(m) == NOTE).unwrap();
        assert!(pin.is_agent_visible(), "the drop pre-pass must spare a pin");
        assert!(
            !out.iter()
                .find(|m| text_of(m) == "q1")
                .unwrap()
                .is_agent_visible(),
            "control: its unpinned neighbour is dropped"
        );
    }

    // ── P3: eligibility and non-duplication ─────────────────────────────────

    /// A pin exempts a message from summarization; it does not also DUPLICATE it
    /// into the summarizer's input. Feeding the summarizer text that also
    /// survives verbatim wastes the window twice over.
    #[tokio::test]
    async fn a_pinned_message_is_not_also_fed_to_the_summarizer() {
        let conversation = conversation_with_an_old_pin(8);
        let provider = provider(100_000);
        let _ = compact_messages_with_window(&provider, &conversation, false, 2)
            .await
            .unwrap();

        let seen = provider.last_summarizer_payload();
        assert!(
            !seen.contains(NOTE),
            "the pinned text must not be summarized as well as kept; payload: {seen}"
        );
        assert!(
            seen.contains("q1"),
            "control: the unpinned prefix WAS summarized"
        );
    }

    /// A pin on a message carrying tool content is NOT honoured: exempting one
    /// half of a tool_request/tool_response pair from a summarization that hides
    /// the other half hands the provider an invalid request. Pins are for
    /// standalone messages, which is the shape of the first consumer.
    #[tokio::test]
    async fn a_pin_on_a_tool_message_is_not_honoured() {
        let mut messages = vec![
            Message::user().with_text("q1"),
            Message::assistant()
                .with_tool_request(
                    "tool_0",
                    Ok(rmcp::model::CallToolRequestParams {
                        task: None,
                        name: "read_file".into(),
                        arguments: None,
                        meta: None,
                    }),
                )
                .pinned(),
            Message::user()
                .with_tool_response(
                    "tool_0",
                    Ok(rmcp::model::CallToolResult {
                        content: vec![rmcp::model::RawContent::text("f").no_annotation()],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    }),
                )
                .pinned(),
        ];
        for i in 2..=8 {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        let conversation = Conversation::new_unvalidated(messages);

        let (compacted, _usage) =
            compact_messages_with_window(&provider(100_000), &conversation, false, 2)
                .await
                .unwrap();

        let agent = compacted.agent_visible_messages();
        assert!(
            !agent.iter().any(|m| m.content.iter().any(|c| matches!(
                c,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            ))),
            "an ineligible pin must not keep half a tool pair alive in the \
             agent's context; agent-visible was: {:#?}",
            agent_texts(&compacted)
        );
        // Ineligible means "treated as an ordinary older message", not
        // "deleted": both halves are still in the user's transcript.
        assert_eq!(
            compacted
                .messages()
                .iter()
                .filter(|m| m.content.iter().any(|c| matches!(
                    c,
                    MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
                )))
                .count(),
            2,
            "the pair stays in the transcript, just not in the agent's context"
        );
    }

    // ── P4: the bound ───────────────────────────────────────────────────────

    /// The pinned set is bounded by construction, or a session accumulating
    /// pins eventually has a conversation that cannot be compacted at all.
    ///
    /// The budget is driven here by the MODEL's context window rather than by an
    /// env var: the share cap resolves against it, so this exercises the real
    /// configured path with no process-global env mutation to race other tests.
    #[tokio::test]
    async fn over_budget_pins_are_evicted_oldest_first_and_announced() {
        // context_limit 1000 -> pinned budget = 1000 * 0.25 = 250 tokens; each
        // ~300-byte note costs ~104. Room for two.
        let filler = "x".repeat(280);
        let mut messages = Vec::new();
        for i in 1..=4 {
            messages.push(
                Message::user()
                    .with_text(format!("pin{i} {filler}"))
                    .pinned(),
            );
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        for i in 5..=8 {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        let conversation = Conversation::new_unvalidated(messages);

        let (compacted, _usage) =
            compact_messages_with_window(&provider(1_000), &conversation, false, 2)
                .await
                .unwrap();

        let pin_visible = |n: usize| {
            compacted
                .messages()
                .iter()
                .find(|m| text_of(m).starts_with(&format!("pin{n} ")))
                .unwrap_or_else(|| panic!("pin{n} missing from the transcript"))
                .is_agent_visible()
        };

        // Newest two honoured, oldest two demoted — the operator's rule.
        assert!(pin_visible(3), "the newest pins keep their exemption");
        assert!(pin_visible(4));
        assert!(!pin_visible(1), "the OLDEST pin is the first to be dropped");
        assert!(!pin_visible(2));

        // ...and the eviction is announced to BOTH audiences, in the
        // transcript, not only in a log line.
        let agent_notice = compacted
            .messages()
            .iter()
            .find(|m| text_of(m).contains("marked to be preserved verbatim"))
            .expect("the model must be told a preserved message was condensed");
        assert!(agent_notice.is_agent_visible() && !agent_notice.is_user_visible());
        assert!(text_of(agent_notice).contains('2'), "names the count");

        let user_notice = compacted
            .messages()
            .iter()
            .filter_map(|m| m.content.iter().find_map(|c| c.as_system_notification()))
            .find(|n| n.msg.contains("could not preserve"))
            .expect("the user who pinned it must be told too");
        assert!(user_notice.msg.contains('2'));
    }

    /// The anti-noise twin: a compaction that honours every pin must not emit an
    /// eviction notice. A notice that appears on the happy path would train
    /// everyone to ignore it.
    #[tokio::test]
    async fn no_eviction_notice_when_every_pin_fits() {
        let conversation = conversation_with_an_old_pin(8);
        let (compacted, _usage) =
            compact_messages_with_window(&provider(100_000), &conversation, false, 2)
                .await
                .unwrap();

        assert!(
            !compacted
                .messages()
                .iter()
                .any(|m| text_of(m).contains("marked to be preserved verbatim")),
            "no eviction happened, so nothing should be announced"
        );
    }

    /// The degenerate shape the bound exists to prevent: a conversation whose
    /// agent-visible content is ENTIRELY pinned. There is genuinely nothing to
    /// summarize, so compaction must return the history untouched rather than
    /// spend a billed round-trip summarizing nothing and tell the model its
    /// context was condensed when it was not.
    #[tokio::test]
    async fn a_wholly_pinned_conversation_is_left_alone_instead_of_summarizing_nothing() {
        let provider = provider(100_000);
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("note one").pinned(),
            Message::user().with_text("note two").pinned(),
        ]);

        let (compacted, _usage) = compact_messages_with_window(&provider, &conversation, false, 0)
            .await
            .unwrap();

        assert_eq!(provider.calls(), 0, "no summarizer round-trip was bought");
        assert_eq!(compacted.messages().len(), 2);
        assert!(compacted.messages().iter().all(|m| m.is_agent_visible()));
    }

    /// The control for the whole feature: without the marker, the same message
    /// in the same place IS summarized away. If this ever passes with the pin
    /// assertions, the tests are measuring "compaction did nothing".
    #[tokio::test]
    async fn an_unpinned_message_in_the_same_place_is_summarized_away() {
        let mut messages = vec![
            Message::user().with_text("q1"),
            Message::assistant().with_text("a1"),
            Message::user().with_text(NOTE),
        ];
        for i in 2..=8 {
            messages.push(Message::user().with_text(format!("q{i}")));
            messages.push(Message::assistant().with_text(format!("a{i}")));
        }
        let conversation = Conversation::new_unvalidated(messages);

        let (compacted, _usage) =
            compact_messages_with_window(&provider(100_000), &conversation, false, 2)
                .await
                .unwrap();

        assert!(
            !find(&compacted, NOTE).is_agent_visible(),
            "an unmarked message must still be summarized away"
        );
    }
}
